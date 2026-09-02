//! Executable zero-overhead contracts for the error channel and execution kernel.
//!
//! This is a **dedicated test binary** on purpose: the counting global allocator is
//! process-wide, so allocation assertions require that no other tests run concurrently.
//! Keep exactly one `#[test]` function here.
//!
//! The contracts, in order of importance:
//! 1. **Layout**: `Result<T, Infallible>` occupies exactly as much space as `T` — the
//!    error channel is free when the error type is uninhabited.
//! 2. **Allocation parity**: running the same queries with an adapter whose error type is
//!    `Infallible` versus an adapter carrying a real error type allocates **exactly the
//!    same number of times**. Choosing a fallible error type must not cost a single byte
//!    of allocation or box until an error is actually produced.
//! 3. **Determinism**: repeated runs allocate identically (no hidden nondeterminism in
//!    pipeline construction).
//! 4. **Laziness**: pulling a single row allocates only a bounded prefix of the full run.

#![deny(warnings)]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use trustfall_core::{
    frontend::parse,
    interpreter::{
        Adapter, AsVertex, ContextIterator, ContextOutcomeIterator, NeighborResolution,
        ResolveEdgeInfo, ResolveInfo, VertexIterator, execution::interpret_ir,
    },
    ir::{EdgeParameters, FieldValue},
    numbers_interpreter::{NumbersAdapter, NumbersVertex},
    schema::Schema,
};

// ---------------------------------------------------------------------------
// Counting allocator
// ---------------------------------------------------------------------------

struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn allocations() -> usize {
    ALLOCATIONS.load(Ordering::Relaxed)
}

fn measure(mut run: impl FnMut()) -> usize {
    // Warm up any lazy process-wide state (e.g. schema parsing caches) by running once
    // before measuring, so measurements reflect steady-state execution.
    run();
    let before = allocations();
    run();
    allocations() - before
}

// ---------------------------------------------------------------------------
// Layout contracts
// ---------------------------------------------------------------------------

const _: () = {
    assert!(
        std::mem::size_of::<Result<FieldValue, std::convert::Infallible>>()
            == std::mem::size_of::<FieldValue>(),
        "Result<FieldValue, Infallible> must be layout-identical to FieldValue"
    );
    assert!(
        std::mem::size_of::<
            Result<
                BTreeMap<Arc<str>, FieldValue>,
                trustfall_core::interpreter::error::ExecutionError<std::convert::Infallible>,
            >,
        >() == std::mem::size_of::<BTreeMap<Arc<str>, FieldValue>>(),
        "rows with an uninhabited error must be layout-identical to plain rows"
    );
    assert!(
        std::mem::size_of::<Result<NumbersVertex, std::convert::Infallible>>()
            == std::mem::size_of::<NumbersVertex>(),
        "Result<NumbersVertex, Infallible> must be layout-identical to NumbersVertex"
    );
};

// ---------------------------------------------------------------------------
// A fallible twin of NumbersAdapter: same behavior, real error type, never fails
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct RealError;

impl std::fmt::Display for RealError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("never actually produced")
    }
}

impl std::error::Error for RealError {}

/// An adapter wrapper: identical structure for both error types, so measuring them
/// against each other isolates exactly what the *error type* costs in the engine.
struct TwinAdapter<E> {
    inner: NumbersAdapter,
    _error: std::marker::PhantomData<fn() -> E>,
}

impl<E> TwinAdapter<E> {
    fn new() -> Self {
        Self { inner: NumbersAdapter::new(), _error: std::marker::PhantomData }
    }
}

#[allow(dead_code)]
type InfallibleTwin = TwinAdapter<std::convert::Infallible>;
type FallibleTwin = TwinAdapter<RealError>;

impl<'a, E> Adapter<'a> for TwinAdapter<E>
where
    E: std::error::Error + 'static,
{
    type Vertex = NumbersVertex;
    type Error = E;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexIterator<'a, Result<Self::Vertex, Self::Error>> {
        Box::new(self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info).map(
            |vertex| match vertex {
                Ok(vertex) => Ok(vertex),
                Err(never) => match never {},
            },
        ))
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, Result<FieldValue, Self::Error>> {
        Box::new(self.inner.resolve_property(contexts, type_name, property_name, resolve_info).map(
            |(ctx, value)| match value {
                Ok(value) => (ctx, Ok(value)),
                Err(never) => match never {},
            },
        ))
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, NeighborResolution<'a, Self::Vertex, Self::Error>> {
        Box::new(
            self.inner
                .resolve_neighbors(contexts, type_name, edge_name, parameters, resolve_info)
                .map(|(ctx, resolution)| match resolution {
                    Ok(iter) => {
                        let lifted: VertexIterator<'a, Result<Self::Vertex, Self::Error>> =
                            Box::new(iter.map(|vertex| match vertex {
                                Ok(vertex) => Ok(vertex),
                                Err(never) => match never {},
                            }));
                        (ctx, Ok(lifted))
                    }
                    Err(never) => match never {},
                }),
        )
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, Result<bool, Self::Error>> {
        Box::new(
            self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info).map(
                |(ctx, value)| match value {
                    Ok(value) => (ctx, Ok(value)),
                    Err(never) => match never {},
                },
            ),
        )
    }
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

const FLAT: &str = r#"{ Number(min: 0, max: 50) { value @output } }"#;
const DEEP: &str = r#"{
    Number(min: 0, max: 30) {
        value @output
        successor @optional {
            ... on Prime {
                prime: value @output
            }
        }
    }
}"#;
const FOLD_RECURSE: &str = r#"{
    Number(min: 1, max: 20) {
        value @output
        multiple(max: 10) @fold {
            factor: value @output
        }
        successor @recurse(depth: 3) {
            succ: value @output
        }
    }
}"#;

fn schema() -> Schema {
    NumbersAdapter::new().schema().clone()
}

fn run_infallible(query: &str) -> usize {
    let indexed = parse(&schema(), query).unwrap();
    let rows: Vec<_> =
        interpret_ir(Arc::new(InfallibleTwin::new()), indexed, Arc::new(BTreeMap::new()))
            .unwrap()
            .map(|row| row.expect("numbers adapter is infallible"))
            .collect();
    rows.len()
}

fn run_fallible(query: &str) -> usize {
    let indexed = parse(&schema(), query).unwrap();
    let rows: Vec<_> =
        interpret_ir(Arc::new(FallibleTwin::new()), indexed, Arc::new(BTreeMap::new()))
            .unwrap()
            .map(|row| row.expect("this adapter never actually fails"))
            .collect();
    rows.len()
}

#[test]
fn error_channel_is_zero_overhead() {
    for query in [FLAT, DEEP, FOLD_RECURSE] {
        // Same behavior.
        let infallible_rows = run_infallible(query);
        let fallible_rows = run_fallible(query);
        assert_eq!(infallible_rows, fallible_rows, "row counts must match for {query}");
        assert!(infallible_rows > 0, "query produced no rows: {query}");

        // Determinism: identical allocation counts across repeated runs.
        let infallible_first = measure(|| {
            run_infallible(query);
        });
        let infallible_second = measure(|| {
            run_infallible(query);
        });
        assert_eq!(
            infallible_first, infallible_second,
            "allocation counts must be deterministic for {query}"
        );

        // Parity: a real error type costs zero additional allocations when no error occurs.
        // Both twins wrap the inner adapter identically, so any count difference is
        // attributable solely to the error type flowing through the engine.
        let fallible_count = measure(|| {
            run_fallible(query);
        });
        assert_eq!(
            infallible_first, fallible_count,
            "fallible error type changed allocation count for {query}: \
             {} vs {}",
            infallible_first, fallible_count,
        );
    }

    // Laziness: one row's work must not resolve the whole input range (50 vertices).
    let indexed = parse(&schema(), FLAT).unwrap();
    let rows =
        interpret_ir(Arc::new(NumbersAdapter::new()), indexed, Arc::new(BTreeMap::new())).unwrap();
    let before = allocations();
    rows.into_iter().take(1).for_each(|row| {
        let _ = row.expect("infallible");
    });
    let lazy_allocs = allocations() - before;

    let indexed = parse(&schema(), FLAT).unwrap();
    let full =
        interpret_ir(Arc::new(NumbersAdapter::new()), indexed, Arc::new(BTreeMap::new())).unwrap();
    let before = allocations();
    full.into_iter().for_each(|row| {
        let _ = row.expect("infallible");
    });
    let full_allocs = allocations() - before;

    assert!(
        lazy_allocs < full_allocs / 2,
        "pulling one row allocated {lazy_allocs} times vs {full_allocs} for the full run: \
         execution is not lazy",
    );
}
