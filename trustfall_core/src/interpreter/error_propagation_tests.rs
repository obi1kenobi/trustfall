//! End-to-end tests for the fallible `FallibleAdapter` error channel.
//!
//! The rest of the test suite uses infallible adapters, so it never exercises the error path.
//! These tests wrap the (infallible) `NumbersAdapter` in a fault-injecting adapter that reports
//! a real error at a configurable point in a configurable resolver method. The focused tests
//! below cover the distinct error boundaries and one composed query shape.

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
        AsVertex, ContextIterator, FallibleAdapter, FallibleContextOutcomeIterator,
        ResolveEdgeInfo, ResolveInfo, VertexIterator, error::ExecutionError,
        execution::interpret_ir,
    },
    ir::{EdgeParameters, FieldValue},
    numbers_interpreter::NumbersAdapter,
};

type Row = BTreeMap<Arc<str>, FieldValue>;

/// Deliberately `!Send + !Sync`: core query execution must also support local-only errors,
/// including errors backed by JavaScript values in WASM adapters.
#[derive(Debug)]
pub(super) struct TestError {
    _local: std::rc::Rc<()>,
}

impl TestError {
    pub(super) fn new() -> Self {
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
pub(super) enum Fault {
    StartingVertices,
    Property,
    Neighbors,
}

/// Wraps `NumbersAdapter` and injects `TestError` into one resolver method once `remaining`
/// successful values have flowed through that method's targeted stream.
pub(super) struct FaultyAdapter {
    inner: NumbersAdapter,
    fault: Fault,
    /// Number of successful values to allow through the faulted stream before erroring.
    remaining: Arc<AtomicUsize>,
    /// Ensures this fixture emits exactly one error.
    error_emitted: Arc<AtomicBool>,
}

impl FaultyAdapter {
    pub(super) fn new(fault: Fault, fail_after: usize) -> Self {
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
        !error_emitted.swap(true, Ordering::SeqCst)
    } else {
        remaining.store(current - 1, Ordering::SeqCst);
        false
    }
}

impl<'a> FallibleAdapter<'a> for FaultyAdapter {
    type Vertex = <NumbersAdapter as FallibleAdapter<'a>>::Vertex;
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
    ) -> FallibleContextOutcomeIterator<'a, V, FieldValue, Self::Error> {
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
    ) -> FallibleContextOutcomeIterator<
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
    ) -> FallibleContextOutcomeIterator<'a, V, bool, Self::Error> {
        Box::new(
            self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info).map(
                |outcome| match outcome {
                    Ok((ctx, value)) => Ok((ctx, value)),
                    Err(never) => match never {},
                },
            ),
        )
    }
}

/// `NumbersAdapter` is infallible, so its outcomes are `Result<_, Infallible>`.
fn unwrap_ok<T>(result: Result<T, std::convert::Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

pub(super) fn run(
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

/// A single resolver fault occupies one row; independent rows continue.
fn assert_single_row_error(results: &[Result<Row, ExecutionError<TestError>>]) {
    let errors = results.iter().filter(|result| result.is_err()).count();
    assert_eq!(errors, 1, "expected exactly one failed row, got {errors}: {results:?}");
}

pub(super) const FLAT: &str = r#"{ Number(min: 0, max: 50) { value @output } }"#;
pub(super) const SUCCESSOR: &str = r#"{ Number(min: 0, max: 50) { successor { value @output } } }"#;

#[test]
fn error_in_resolve_starting_vertices_becomes_a_row_error() {
    assert!(baseline_row_count(FLAT) > 5);
    let results = run(Fault::StartingVertices, 5, FLAT);
    assert_single_row_error(&results);
}

#[test]
fn error_in_resolve_property_becomes_a_row_error() {
    assert!(baseline_row_count(FLAT) > 3);
    let results = run(Fault::Property, 3, FLAT);
    assert_single_row_error(&results);
}

#[test]
fn error_in_resolve_neighbors_becomes_a_row_error() {
    assert!(baseline_row_count(SUCCESSOR) > 4);
    let results = run(Fault::Neighbors, 4, SUCCESSOR);
    assert_single_row_error(&results);
}

#[test]
fn error_inside_fold_becomes_a_row_error() {
    let query = r#"{
        Number(min: 1, max: 50) {
            value @output
            multiple(max: 30) @fold {
                factor: value @output
            }
        }
    }"#;
    let results = run(Fault::Neighbors, 5, query);
    assert_single_row_error(&results);
}

/// Laziness with errors: pulling one row runs only enough adapter work for that row.
#[test]
fn partial_consumption_is_lazy() {
    let adapter = Arc::new(FaultyAdapter::new(Fault::Property, usize::MAX));
    let schema = adapter.inner.schema().clone();
    let indexed = parse(&schema, FLAT).unwrap();
    let mut rows = interpret_ir(adapter.clone(), indexed, Arc::new(BTreeMap::new())).unwrap();

    // Property budget starts at usize::MAX; after one row it must have decreased by
    // only a handful of resolutions (one per output in that row's pipeline prefix).
    let row = rows.next().expect("at least one row");
    assert!(row.is_ok());
    let consumed = usize::MAX - adapter.remaining.load(Ordering::SeqCst);
    // The flat query resolves one property per starting vertex for the row it emitted,
    // plus the pipeline may prefetch a bounded amount for the next row. 32 is a very
    // generous ceiling that still catches eager whole-batch resolution.
    assert!(
        consumed <= 32,
        "pulling one row consumed {consumed} property resolutions; execution is not lazy"
    );
}
