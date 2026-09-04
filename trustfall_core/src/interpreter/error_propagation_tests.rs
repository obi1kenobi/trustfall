//! End-to-end tests for the fallible `Adapter` error channel.
//!
//! The rest of the test suite uses infallible adapters, so it never exercises the error path.
//! These tests wrap the (infallible) `NumbersAdapter` in a fault-injecting adapter that reports
//! a real error at a configurable point in a configurable resolver method, and assert the
//! engine's fail-fast contract:
//! - successful results produced *before* the error are still yielded,
//! - then exactly one `Err` is yielded,
//! - then the stream ends (nothing after the error).

use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use crate::{
    frontend::parse,
    interpreter::{
        Adapter, AsVertex, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, ResolveInfo,
        VertexIterator, error::ExecutionError, execution::interpret_ir,
    },
    ir::{EdgeParameters, FieldValue},
    numbers_interpreter::NumbersAdapter,
};

type Row = BTreeMap<Arc<str>, FieldValue>;

/// Deliberately `!Send + !Sync`: core query execution must also support local-only errors,
/// including errors backed by JavaScript values in WASM adapters.
#[derive(Debug)]
struct TestError {
    _local: std::rc::Rc<()>,
}

impl TestError {
    fn new() -> Self {
        Self { _local: std::rc::Rc::new(()) }
    }
}

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "injected test error")
    }
}

impl std::error::Error for TestError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fault {
    StartingVertices,
    Property,
    Neighbors,
    Coercion,
}

/// Wraps `NumbersAdapter` and injects `TestError` into one resolver method once `remaining`
/// successful values have flowed through that method's targeted stream.
struct FaultyAdapter {
    inner: NumbersAdapter,
    fault: Fault,
    /// Number of successful values to allow through the faulted stream before erroring.
    remaining: Arc<AtomicUsize>,
    /// Any poll after the injected error is a fail-fast contract violation.
    error_emitted: Arc<AtomicBool>,
}

impl FaultyAdapter {
    fn new(fault: Fault, fail_after: usize) -> Self {
        Self {
            inner: NumbersAdapter::new(),
            fault,
            remaining: Arc::new(AtomicUsize::new(fail_after)),
            error_emitted: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Returns `true` when it is time to inject an error: `false` (decrementing the budget) while
/// there is remaining budget, `true` once the budget is exhausted.
fn should_error(remaining: &AtomicUsize, error_emitted: &AtomicBool) -> bool {
    let current = remaining.load(Ordering::SeqCst);
    if current == 0 {
        assert!(
            !error_emitted.swap(true, Ordering::SeqCst),
            "adapter was polled after its first error"
        );
        true
    } else {
        remaining.store(current - 1, Ordering::SeqCst);
        false
    }
}

impl<'a> Adapter<'a> for FaultyAdapter {
    type Vertex = <NumbersAdapter as Adapter<'a>>::Vertex;
    type Error = TestError;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexIterator<'a, Result<Self::Vertex, Self::Error>> {
        let inner = self
            .inner
            .resolve_starting_vertices(edge_name, parameters, resolve_info)
            .map(unwrap_ok);
        if self.fault == Fault::StartingVertices {
            let remaining = self.remaining.clone();
            let error_emitted = self.error_emitted.clone();
            Box::new(inner.map(move |v| {
                if should_error(&remaining, &error_emitted) { Err(TestError::new()) } else { Ok(v) }
            }))
        } else {
            Box::new(inner.map(Ok))
        }
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, FieldValue, Self::Error> {
        let inner = self
            .inner
            .resolve_property(contexts, type_name, property_name, resolve_info)
            .map(|outcome| match outcome {
                Ok((ctx, value)) => (ctx, value),
                Err(never) => match never {},
            });
        if self.fault == Fault::Property {
            let remaining = self.remaining.clone();
            let error_emitted = self.error_emitted.clone();
            Box::new(inner.map(move |(ctx, value)| {
                if should_error(&remaining, &error_emitted) {
                    Err(TestError::new())
                } else {
                    Ok((ctx, value))
                }
            }))
        } else {
            Box::new(inner.map(Ok))
        }
    }

    #[allow(clippy::type_complexity)]
    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<
        'a,
        V,
        VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
        Self::Error,
    > {
        let faulted = self.fault == Fault::Neighbors;
        let remaining = self.remaining.clone();
        let error_emitted = self.error_emitted.clone();
        let inner =
            self.inner.resolve_neighbors(contexts, type_name, edge_name, parameters, resolve_info);
        Box::new(inner.map(move |outcome| match outcome {
            Ok((ctx, neighbors)) => {
                let neighbors = neighbors.map(unwrap_ok);
                let out: VertexIterator<'a, Result<Self::Vertex, Self::Error>> = if faulted {
                    let remaining = remaining.clone();
                    let error_emitted = error_emitted.clone();
                    Box::new(neighbors.map(move |v| {
                        if should_error(&remaining, &error_emitted) {
                            Err(TestError::new())
                        } else {
                            Ok(v)
                        }
                    }))
                } else {
                    Box::new(neighbors.map(Ok))
                };
                Ok((ctx, out))
            }
            Err(never) => match never {},
        }))
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, bool, Self::Error> {
        let inner = self
            .inner
            .resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
            .map(|outcome| match outcome {
                Ok((ctx, value)) => (ctx, value),
                Err(never) => match never {},
            });
        if self.fault == Fault::Coercion {
            let remaining = self.remaining.clone();
            let error_emitted = self.error_emitted.clone();
            Box::new(inner.map(move |(ctx, value)| {
                if should_error(&remaining, &error_emitted) {
                    Err(TestError::new())
                } else {
                    Ok((ctx, value))
                }
            }))
        } else {
            Box::new(inner.map(Ok))
        }
    }
}

/// `NumbersAdapter` is infallible, so its outcomes are `Result<_, Infallible>`.
fn unwrap_ok<T>(result: Result<T, std::convert::Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

fn run(
    fault: Fault,
    fail_after: usize,
    query: &str,
) -> Vec<Result<Row, ExecutionError<TestError>>> {
    let adapter = Arc::new(FaultyAdapter::new(fault, fail_after));
    let schema = adapter.inner.schema().clone();
    let indexed = parse(&schema, query).expect("query failed to parse");
    interpret_ir(adapter, indexed, Arc::new(BTreeMap::new()))
        .expect("unexpected query arguments error")
        .collect()
}

/// Baseline row count for a query run against a never-erroring adapter.
fn baseline_row_count(query: &str) -> usize {
    // A huge budget means the fault never triggers; assert that and count the rows.
    let results = run(Fault::Property, usize::MAX, query);
    assert!(results.iter().all(Result::is_ok), "baseline run unexpectedly errored");
    results.len()
}

/// Assert fail-fast: exactly `expected_ok` successful rows, then exactly one `Err`, then nothing.
fn assert_fail_fast_exact(results: &[Result<Row, ExecutionError<TestError>>], expected_ok: usize) {
    assert_eq!(
        results.len(),
        expected_ok + 1,
        "expected {expected_ok} Ok rows then exactly one Err; got {} items",
        results.len()
    );
    for (i, r) in results.iter().take(expected_ok).enumerate() {
        assert!(r.is_ok(), "result {i} should be Ok, was {r:?}");
    }
    assert!(
        matches!(results[expected_ok], Err(ExecutionError::Adapter(TestError { .. }))),
        "final item should be the injected adapter error, was {:?}",
        results[expected_ok],
    );
}

/// Assert fail-fast structurally (for queries where the exact successful-row count is hard to
/// predict): the last item is the injected error, and it is the *only* error.
fn assert_fail_fast_terminal(results: &[Result<Row, ExecutionError<TestError>>]) {
    assert!(!results.is_empty(), "expected at least the terminal error");
    let (last, rest) = results.split_last().unwrap();
    assert!(
        matches!(last, Err(ExecutionError::Adapter(TestError { .. }))),
        "last item should be the injected adapter error, was {last:?}"
    );
    for (i, r) in rest.iter().enumerate() {
        assert!(r.is_ok(), "item {i} before the error should be Ok, was {r:?}");
    }
}

const FLAT: &str = r#"{ Number(min: 0, max: 50) { value @output } }"#;
const SUCCESSOR: &str = r#"{ Number(min: 0, max: 50) { successor { value @output } } }"#;

#[test]
fn error_in_resolve_starting_vertices_is_fail_fast() {
    assert!(baseline_row_count(FLAT) > 5);
    let results = run(Fault::StartingVertices, 5, FLAT);
    assert_fail_fast_exact(&results, 5);
}

#[test]
fn error_in_resolve_property_is_fail_fast() {
    assert!(baseline_row_count(FLAT) > 3);
    let results = run(Fault::Property, 3, FLAT);
    assert_fail_fast_exact(&results, 3);
}

#[test]
fn error_in_resolve_neighbors_is_fail_fast() {
    assert!(baseline_row_count(SUCCESSOR) > 4);
    let results = run(Fault::Neighbors, 4, SUCCESSOR);
    assert_fail_fast_exact(&results, 4);
}

#[test]
fn error_after_zero_rows_yields_only_the_error() {
    let results = run(Fault::StartingVertices, 0, FLAT);
    assert_fail_fast_exact(&results, 0);
}

#[test]
fn no_error_when_budget_exceeds_work() {
    // If the fault never triggers, the output matches an infallible run exactly.
    let total = baseline_row_count(FLAT);
    let results = run(Fault::Property, total + 10, FLAT);
    assert_eq!(results.len(), total);
    assert!(results.iter().all(Result::is_ok));
}

#[test]
fn error_in_coercion_is_fail_fast() {
    let query = r#"{ Number(min: 0, max: 50) { successor { ... on Prime { value @output } } } }"#;
    let results = run(Fault::Coercion, 4, query);
    assert_fail_fast_terminal(&results);
}

#[test]
fn error_inside_fold_terminates() {
    // The fold eagerly materializes `multiple` neighbors; an error mid-fold must terminate.
    let query = r#"{
        Number(min: 1, max: 50) {
            value @output
            multiple(max: 30) @fold {
                factor: value @output
            }
        }
    }"#;
    let results = run(Fault::Neighbors, 5, query);
    assert_fail_fast_terminal(&results);
}

#[test]
fn scalar_error_inside_materialized_fold_stops_adapter_polls() {
    // Fold materialization is eager relative to yielding the parent row. A scalar resolver error
    // inside it must cancel the rest of that materialization immediately, rather than continuing
    // to pull adapter data before the outer result iterator gets a chance to observe the error.
    let query = r#"{
        Number(min: 1, max: 50) {
            multiple(max: 30) @fold {
                factor: value @output
            }
        }
    }"#;
    let results = run(Fault::Property, 5, query);
    assert_fail_fast_terminal(&results);
}

#[test]
fn error_inside_recurse_terminates() {
    let query = r#"{
        Number(min: 0, max: 10) {
            value @output
            successor @recurse(depth: 3) {
                succ: value @output
            }
        }
    }"#;
    let results = run(Fault::Neighbors, 5, query);
    assert_fail_fast_terminal(&results);
}

#[test]
fn error_inside_optional_terminates() {
    let query = r#"{
        Number(min: 0, max: 50) {
            value @output
            predecessor @optional {
                pred: value @output
            }
        }
    }"#;
    let results = run(Fault::Neighbors, 5, query);
    assert_fail_fast_terminal(&results);
}
