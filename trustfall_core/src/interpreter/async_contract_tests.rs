//! Async-engine tests for:
//! - fail-fast error propagation (native `Result` path, mirroring sync `error_propagation_tests`)
//! - true streaming of [`SyncToAsyncAdapter`] / context consumption
//! - bounded concurrent helpers (order preservation)
//! - adapter contract violations (too few / too many outcomes; mid-stream neighbor `Err`)

use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use futures_util::{StreamExt, stream};

use crate::{
    frontend::parse,
    interpreter::{
        AsVertex, DataContext, ResolveEdgeInfo, ResolveInfo,
        async_adapter::{AsyncAdapter, ContextOutcomeStream, ContextStream, VertexStream},
        async_helpers::{map_contexts_buffered, try_resolve_property_with_concurrent},
        async_test_adapter::SyncToAsyncAdapter,
        engine::interpret_ir as interpret_ir_async,
        error::ExecutionError,
    },
    ir::{EdgeParameters, FieldValue},
    numbers_interpreter::NumbersAdapter,
};

type Row = BTreeMap<Arc<str>, FieldValue>;

#[derive(Debug, PartialEq, Eq, Clone)]
struct TestError;

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

/// Async fault injector over [`NumbersAdapter`] via streaming [`SyncToAsyncAdapter`].
struct FaultyAsyncAdapter {
    inner: SyncToAsyncAdapter<NumbersAdapter>,
    fault: Fault,
    remaining: Arc<AtomicUsize>,
}

impl FaultyAsyncAdapter {
    fn new(fault: Fault, fail_after: usize) -> Self {
        Self {
            inner: SyncToAsyncAdapter::new(Arc::new(NumbersAdapter::new())),
            fault,
            remaining: Arc::new(AtomicUsize::new(fail_after)),
        }
    }

    fn schema(&self) -> &crate::schema::Schema {
        self.inner.inner().schema()
    }
}

fn should_error(remaining: &AtomicUsize) -> bool {
    let current = remaining.load(Ordering::SeqCst);
    if current == 0 {
        true
    } else {
        remaining.store(current - 1, Ordering::SeqCst);
        false
    }
}

fn unwrap_ok<T>(result: Result<T, std::convert::Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

impl<'a> AsyncAdapter<'a> for FaultyAsyncAdapter {
    type Vertex = <NumbersAdapter as crate::interpreter::Adapter<'a>>::Vertex;
    type Error = TestError;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexStream<'a, Result<Self::Vertex, Self::Error>> {
        let inner = self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info);
        if self.fault == Fault::StartingVertices {
            let remaining = self.remaining.clone();
            Box::pin(inner.map(move |item| {
                let v = unwrap_ok(item);
                if should_error(&remaining) { Err(TestError) } else { Ok(v) }
            }))
        } else {
            Box::pin(inner.map(|item| Ok(unwrap_ok(item))))
        }
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextStream<'a, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'a, V, Result<FieldValue, Self::Error>> {
        let outcomes =
            self.inner.resolve_property(contexts, type_name, property_name, resolve_info);
        if self.fault == Fault::Property {
            let remaining = self.remaining.clone();
            Box::pin(outcomes.map(move |(ctx, value)| {
                let value = unwrap_ok(value);
                if should_error(&remaining) { (ctx, Err(TestError)) } else { (ctx, Ok(value)) }
            }))
        } else {
            Box::pin(outcomes.map(|(ctx, value)| (ctx, Ok(unwrap_ok(value)))))
        }
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextStream<'a, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeStream<'a, V, VertexStream<'a, Result<Self::Vertex, Self::Error>>> {
        let faulted = self.fault == Fault::Neighbors;
        let remaining = self.remaining.clone();
        let outcomes =
            self.inner.resolve_neighbors(contexts, type_name, edge_name, parameters, resolve_info);
        Box::pin(outcomes.map(move |(ctx, neighbors)| {
            let neighbors = neighbors.map(unwrap_ok);
            let out: VertexStream<'a, Result<Self::Vertex, Self::Error>> =
                if faulted {
                    let remaining = remaining.clone();
                    Box::pin(neighbors.map(move |v| {
                        if should_error(&remaining) { Err(TestError) } else { Ok(v) }
                    }))
                } else {
                    Box::pin(neighbors.map(Ok))
                };
            (ctx, out)
        }))
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextStream<'a, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'a, V, Result<bool, Self::Error>> {
        let outcomes =
            self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info);
        if self.fault == Fault::Coercion {
            let remaining = self.remaining.clone();
            Box::pin(outcomes.map(move |(ctx, value)| {
                let value = unwrap_ok(value);
                if should_error(&remaining) { (ctx, Err(TestError)) } else { (ctx, Ok(value)) }
            }))
        } else {
            Box::pin(outcomes.map(|(ctx, value)| (ctx, Ok(unwrap_ok(value)))))
        }
    }
}

fn run_async(
    fault: Fault,
    fail_after: usize,
    query: &str,
) -> Vec<Result<Row, ExecutionError<TestError>>> {
    let adapter = Arc::new(FaultyAsyncAdapter::new(fault, fail_after));
    let schema = adapter.schema().clone();
    let indexed = parse(&schema, query).expect("query failed to parse");
    let stream = interpret_ir_async(adapter, indexed, Arc::new(BTreeMap::new()))
        .expect("unexpected query arguments error");
    futures_executor::block_on(async { stream.collect().await })
}

fn baseline_row_count(query: &str) -> usize {
    let results = run_async(Fault::Property, usize::MAX, query);
    assert!(results.iter().all(Result::is_ok), "baseline run unexpectedly errored");
    results.len()
}

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
        matches!(results[expected_ok], Err(ExecutionError::Adapter(TestError))),
        "final item should be the injected adapter error, was {:?}",
        results[expected_ok],
    );
}

fn assert_fail_fast_terminal(results: &[Result<Row, ExecutionError<TestError>>]) {
    assert!(!results.is_empty(), "expected at least the terminal error");
    let (last, rest) = results.split_last().unwrap();
    assert!(
        matches!(last, Err(ExecutionError::Adapter(TestError))),
        "last item should be the injected adapter error, was {last:?}"
    );
    for (i, r) in rest.iter().enumerate() {
        assert!(r.is_ok(), "item {i} before the error should be Ok, was {r:?}");
    }
}

const FLAT: &str = r#"{ Number(min: 0, max: 50) { value @output } }"#;
const SUCCESSOR: &str = r#"{ Number(min: 0, max: 50) { successor { value @output } } }"#;

// --- Fail-fast error propagation (async native Result path) ---

#[test]
fn async_error_in_starting_vertices_is_fail_fast() {
    assert!(baseline_row_count(FLAT) > 5);
    let results = run_async(Fault::StartingVertices, 5, FLAT);
    assert_fail_fast_exact(&results, 5);
}

#[test]
fn async_error_in_property_is_fail_fast() {
    assert!(baseline_row_count(FLAT) > 3);
    let results = run_async(Fault::Property, 3, FLAT);
    assert_fail_fast_exact(&results, 3);
}

#[test]
fn async_error_in_neighbors_is_fail_fast() {
    assert!(baseline_row_count(SUCCESSOR) > 4);
    let results = run_async(Fault::Neighbors, 4, SUCCESSOR);
    assert_fail_fast_exact(&results, 4);
}

#[test]
fn async_error_after_zero_rows_yields_only_the_error() {
    let results = run_async(Fault::StartingVertices, 0, FLAT);
    assert_fail_fast_exact(&results, 0);
}

#[test]
fn async_error_in_coercion_is_fail_fast() {
    let query = r#"{ Number(min: 0, max: 50) { successor { ... on Prime { value @output } } } }"#;
    let results = run_async(Fault::Coercion, 4, query);
    assert_fail_fast_terminal(&results);
}

#[test]
fn async_error_inside_fold_terminates() {
    let query = r#"{
        Number(min: 1, max: 50) {
            value @output
            multiple(max: 30) @fold {
                factor: value @output
            }
        }
    }"#;
    let results = run_async(Fault::Neighbors, 5, query);
    assert_fail_fast_terminal(&results);
}

#[test]
fn async_error_inside_recurse_terminates() {
    let query = r#"{
        Number(min: 0, max: 10) {
            value @output
            successor @recurse(depth: 3) {
                succ: value @output
            }
        }
    }"#;
    let results = run_async(Fault::Neighbors, 5, query);
    assert_fail_fast_terminal(&results);
}

#[test]
fn async_error_inside_optional_terminates() {
    let query = r#"{
        Number(min: 0, max: 50) {
            value @output
            predecessor @optional {
                pred: value @output
            }
        }
    }"#;
    let results = run_async(Fault::Neighbors, 5, query);
    assert_fail_fast_terminal(&results);
}

// --- Mid-stream neighbor Err (contract: fail-fast, no trailing Ok) ---

#[test]
fn mid_stream_neighbor_err_fails_fast() {
    // Neighbor stream for each parent is successor (one neighbor). Fail on the 3rd neighbor
    // item overall so some Ok rows exist before the terminal Err.
    let results = run_async(Fault::Neighbors, 3, SUCCESSOR);
    assert_fail_fast_exact(&results, 3);
}

// --- True streaming: do not collect the full input before first outcome ---

#[test]
fn sync_to_async_streams_one_context_at_a_time() {
    // Sync adapter that panics if resolve_property is called with more than one context.
    use std::num::NonZeroUsize;

    use crate::interpreter::{Adapter, ContextIterator, ContextOutcomeIterator, VertexIterator};

    struct BatchSizeAssertAdapter;

    impl<'a> Adapter<'a> for BatchSizeAssertAdapter {
        type Vertex = u64;
        type Error = std::convert::Infallible;

        fn resolve_starting_vertices(
            &self,
            _edge_name: &Arc<str>,
            _parameters: &EdgeParameters,
            _resolve_info: &ResolveInfo,
        ) -> VertexIterator<'a, Result<Self::Vertex, Self::Error>> {
            Box::new(std::iter::empty())
        }

        fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
            &self,
            contexts: ContextIterator<'a, V>,
            _type_name: &Arc<str>,
            _property_name: &Arc<str>,
            _resolve_info: &ResolveInfo,
        ) -> ContextOutcomeIterator<'a, V, Result<FieldValue, Self::Error>> {
            let items: Vec<_> = contexts.collect();
            assert_eq!(
                items.len(),
                1,
                "SyncToAsyncAdapter must stream one context at a time, got batch of {}",
                items.len()
            );
            Box::new(items.into_iter().map(|ctx| {
                let v = ctx.active_vertex::<u64>().copied().unwrap_or(0);
                (ctx, Ok(FieldValue::Int64(v as i64)))
            }))
        }

        fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
            &self,
            contexts: ContextIterator<'a, V>,
            _type_name: &Arc<str>,
            _edge_name: &Arc<str>,
            _parameters: &EdgeParameters,
            _resolve_info: &ResolveEdgeInfo,
        ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Result<Self::Vertex, Self::Error>>>
        {
            Box::new(contexts.map(|ctx| {
                let empty: VertexIterator<'a, Result<Self::Vertex, Self::Error>> =
                    Box::new(std::iter::empty());
                (ctx, empty)
            }))
        }

        fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
            &self,
            contexts: ContextIterator<'a, V>,
            _type_name: &Arc<str>,
            _coerce_to_type: &Arc<str>,
            _resolve_info: &ResolveInfo,
        ) -> ContextOutcomeIterator<'a, V, Result<bool, Self::Error>> {
            Box::new(contexts.map(|ctx| (ctx, Ok(false))))
        }
    }

    let adapter = SyncToAsyncAdapter::new(Arc::new(BatchSizeAssertAdapter));
    let contexts: ContextStream<'_, u64> =
        Box::pin(stream::iter((0u64..5).map(|v| DataContext::new(Some(v)))));
    let type_name: Arc<str> = Arc::from("T");
    let prop: Arc<str> = Arc::from("p");

    // Drive resolve_property with a ResolveInfo from a real numbers parse (only used for hints).
    let numbers = NumbersAdapter::new();
    let schema = numbers.schema().clone();
    let indexed = parse(&schema, r#"{ Zero { value @output } }"#).unwrap();
    let args = Arc::new(BTreeMap::new());
    let query = crate::interpreter::InterpretedQuery::from_query_and_arguments(indexed, args)
        .expect("args");
    let vid = crate::ir::Vid::new(NonZeroUsize::new(1).unwrap());
    let resolve_info = ResolveInfo::new(query, vid, false);

    let outcomes = adapter.resolve_property(contexts, &type_name, &prop, &resolve_info);
    let collected: Vec<_> = futures_executor::block_on(async { outcomes.collect().await });
    assert_eq!(collected.len(), 5);
    for (i, (_ctx, value)) in collected.iter().enumerate() {
        assert_eq!(value, &Ok(FieldValue::Int64(i as i64)));
    }
}

// --- Bounded concurrent helpers: order preservation ---

#[test]
fn map_contexts_buffered_preserves_order_under_concurrency() {
    let contexts: ContextStream<'_, u64> =
        Box::pin(stream::iter((0u64..20).map(|v| DataContext::new(Some(v)))));
    // Deliberately reverse-ish completion times: larger ids finish first if unordered.
    let out = map_contexts_buffered(contexts, 8, |ctx| {
        let id = *ctx.active_vertex::<u64>().unwrap();
        async move {
            // Yield to the executor so buffered concurrency interleaves.
            for _ in 0..(20 - id) {
                futures_util::task::noop_waker();
                // Tiny cooperative yield via ready future chain.
                let _ = futures_util::future::ready(()).await;
            }
            (ctx, id)
        }
    });
    let values: Vec<u64> =
        futures_executor::block_on(async { out.map(|(_ctx, id)| id).collect().await });
    assert_eq!(values, (0u64..20).collect::<Vec<_>>());
}

#[test]
fn try_resolve_property_concurrent_preserves_order_and_nulls() {
    let contexts: ContextStream<'_, u64> = Box::pin(stream::iter(vec![
        DataContext::new(Some(1u64)),
        DataContext::new(None),
        DataContext::new(Some(3u64)),
    ]));
    let out = try_resolve_property_with_concurrent(contexts, 4, |v: u64| async move {
        Ok::<_, TestError>(FieldValue::Int64(v as i64 * 10))
    });
    let values: Vec<Result<FieldValue, TestError>> =
        futures_executor::block_on(async { out.map(|(_c, v)| v).collect().await });
    assert_eq!(
        values,
        vec![Ok(FieldValue::Int64(10)), Ok(FieldValue::Null), Ok(FieldValue::Int64(30)),]
    );
}

// --- Contract violations: too few / too many outcomes ---

/// Adapter that yields fewer property outcomes than input contexts.
struct TooFewOutcomesAdapter {
    yield_count: usize,
}

impl<'a> AsyncAdapter<'a> for TooFewOutcomesAdapter {
    type Vertex = u64;
    type Error = TestError;

    fn resolve_starting_vertices(
        &self,
        _edge_name: &Arc<str>,
        _parameters: &EdgeParameters,
        _resolve_info: &ResolveInfo,
    ) -> VertexStream<'a, Result<Self::Vertex, Self::Error>> {
        Box::pin(stream::iter((0u64..5).map(Ok)))
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextStream<'a, V>,
        _type_name: &Arc<str>,
        _property_name: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'a, V, Result<FieldValue, Self::Error>> {
        let limit = self.yield_count;
        Box::pin(async_stream::stream! {
            let mut contexts = contexts;
            let mut n = 0usize;
            while let Some(ctx) = contexts.next().await {
                if n >= limit {
                    // Drop remaining contexts without outcomes — contract violation.
                    break;
                }
                n += 1;
                yield (ctx, Ok(FieldValue::Int64(n as i64)));
            }
        })
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextStream<'a, V>,
        _type_name: &Arc<str>,
        _edge_name: &Arc<str>,
        _parameters: &EdgeParameters,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeStream<'a, V, VertexStream<'a, Result<Self::Vertex, Self::Error>>> {
        Box::pin(contexts.map(|ctx| {
            let empty: VertexStream<'a, Result<Self::Vertex, Self::Error>> =
                Box::pin(stream::empty());
            (ctx, empty)
        }))
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextStream<'a, V>,
        _type_name: &Arc<str>,
        _coerce_to_type: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'a, V, Result<bool, Self::Error>> {
        Box::pin(contexts.map(|ctx| (ctx, Ok(true))))
    }
}

/// Adapter that yields *extra* property outcomes beyond the input contexts.
struct TooManyOutcomesAdapter;

impl<'a> AsyncAdapter<'a> for TooManyOutcomesAdapter {
    type Vertex = u64;
    type Error = TestError;

    fn resolve_starting_vertices(
        &self,
        _edge_name: &Arc<str>,
        _parameters: &EdgeParameters,
        _resolve_info: &ResolveInfo,
    ) -> VertexStream<'a, Result<Self::Vertex, Self::Error>> {
        Box::pin(stream::iter((0u64..2).map(Ok)))
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextStream<'a, V>,
        _type_name: &Arc<str>,
        _property_name: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'a, V, Result<FieldValue, Self::Error>> {
        Box::pin(async_stream::stream! {
            let mut contexts = contexts;
            let mut last: Option<DataContext<V>> = None;
            while let Some(ctx) = contexts.next().await {
                yield (ctx.clone(), Ok(FieldValue::Int64(1)));
                last = Some(ctx);
            }
            // Extra phantom outcome — contract violation.
            if let Some(ctx) = last {
                yield (ctx, Ok(FieldValue::Int64(999)));
            }
        })
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextStream<'a, V>,
        _type_name: &Arc<str>,
        _edge_name: &Arc<str>,
        _parameters: &EdgeParameters,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeStream<'a, V, VertexStream<'a, Result<Self::Vertex, Self::Error>>> {
        Box::pin(contexts.map(|ctx| {
            let empty: VertexStream<'a, Result<Self::Vertex, Self::Error>> =
                Box::pin(stream::empty());
            (ctx, empty)
        }))
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextStream<'a, V>,
        _type_name: &Arc<str>,
        _coerce_to_type: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'a, V, Result<bool, Self::Error>> {
        Box::pin(contexts.map(|ctx| (ctx, Ok(true))))
    }
}

#[test]
fn contract_too_few_property_outcomes_truncates_results() {
    // Documented engine behavior: it does not pad missing outcomes; the stage ends early
    // with fewer outcomes than input contexts. This is a silent contract violation by the adapter.
    use std::num::NonZeroUsize;

    use super::engine::{FallibleContextStream, begin_stage, finish_stage};

    let numbers = NumbersAdapter::new();
    let schema = numbers.schema().clone();
    let input: FallibleContextStream<'_, u64, TestError> =
        Box::pin(stream::iter((0u64..5).map(|v| Ok(DataContext::new(Some(v))))));
    let (plain, upstream) = begin_stage(input);
    let too_few = TooFewOutcomesAdapter { yield_count: 2 };
    let type_name: Arc<str> = Arc::from("Number");
    let prop: Arc<str> = Arc::from("value");
    let indexed = parse(&schema, r#"{ Number(min: 0, max: 4) { value @output } }"#).unwrap();
    let query = crate::interpreter::InterpretedQuery::from_query_and_arguments(
        indexed,
        Arc::new(BTreeMap::new()),
    )
    .unwrap();
    let vid = crate::ir::Vid::new(NonZeroUsize::new(1).unwrap());
    let resolve_info = ResolveInfo::new(query, vid, false);
    let outcomes = too_few.resolve_property(plain, &type_name, &prop, &resolve_info);
    let staged = finish_stage(outcomes, upstream);
    let items: Vec<_> = futures_executor::block_on(async { staged.collect().await });
    assert_eq!(items.len(), 2, "too-few outcomes truncates; got {items:?}");
    assert!(items.iter().all(|r| r.is_ok()));
}

#[test]
fn contract_too_many_property_outcomes_are_accepted() {
    // Documented engine behavior: extra outcomes are not rejected; they flow through as extra
    // items. Adapter authors must not rely on the engine to enforce the 1:1 contract at runtime.
    use std::num::NonZeroUsize;

    use super::engine::{FallibleContextStream, begin_stage, finish_stage};

    let numbers = NumbersAdapter::new();
    let schema = numbers.schema().clone();
    let input: FallibleContextStream<'_, u64, TestError> =
        Box::pin(stream::iter((0u64..2).map(|v| Ok(DataContext::new(Some(v))))));
    let (plain, upstream) = begin_stage(input);
    let adapter = TooManyOutcomesAdapter;
    let type_name: Arc<str> = Arc::from("Number");
    let prop: Arc<str> = Arc::from("value");
    let indexed = parse(&schema, r#"{ Number(min: 0, max: 1) { value @output } }"#).unwrap();
    let query = crate::interpreter::InterpretedQuery::from_query_and_arguments(
        indexed,
        Arc::new(BTreeMap::new()),
    )
    .unwrap();
    let vid = crate::ir::Vid::new(NonZeroUsize::new(1).unwrap());
    let resolve_info = ResolveInfo::new(query, vid, false);
    let outcomes = adapter.resolve_property(plain, &type_name, &prop, &resolve_info);
    let staged = finish_stage(outcomes, upstream);
    let items: Vec<_> = futures_executor::block_on(async { staged.collect().await });
    assert_eq!(items.len(), 3, "too-many outcomes produce extra items; got {items:?}");
    assert!(items.iter().all(|r| r.is_ok()));
}
