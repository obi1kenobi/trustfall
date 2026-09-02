//! Execution benchmarks for the shared stream kernel.
//!
//! Run with: `cargo bench -p trustfall_core --features __private`
//! (the numbers interpreter used as the data source is feature-gated).
//!
//! What these benches guard:
//! - **Sync frontend cost** across query shapes (flat, optional+coercion, fold+recurse).
//! - **Error-channel parity**: an adapter carrying a real error type runs at the same
//!   speed as the identical adapter with `Error = Infallible` (also asserted
//!   allocation-exactly in `tests/zero_overhead.rs`).
//! - **Async frontend cost**: the same query on the native async route, driven to
//!   completion with a simple block-on executor.

use std::{collections::BTreeMap, sync::Arc};

use criterion::{Criterion, criterion_group, criterion_main};
use trustfall_core::{
    frontend::parse,
    interpreter::{
        async_test_adapter::SyncToAsyncAdapter, execution::interpret_ir, interpret_ir_async,
    },
    numbers_interpreter::NumbersAdapter,
    schema::Schema,
};

const FLAT: &str = r#"{ Number(min: 0, max: 1000) { value @output } }"#;
const DEEP: &str = r#"{
    Number(min: 0, max: 500) {
        value @output
        successor @optional {
            ... on Prime {
                prime: value @output
            }
        }
    }
}"#;
const FOLD_RECURSE: &str = r#"{
    Number(min: 1, max: 300) {
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

fn bench_sync(c: &mut Criterion) {
    let schema = schema();
    for (name, query) in [("flat", FLAT), ("deep", DEEP), ("fold_recurse", FOLD_RECURSE)] {
        let indexed = parse(&schema, query).unwrap();
        c.bench_function(&format!("sync/{name}"), |b| {
            b.iter(|| {
                let rows: Vec<_> = interpret_ir(
                    Arc::new(NumbersAdapter::new()),
                    indexed.clone(),
                    Arc::new(BTreeMap::new()),
                )
                .unwrap()
                .map(|row| row.expect("numbers adapter is infallible"))
                .collect();
                assert!(!rows.is_empty());
                rows.len()
            })
        });
    }
}

fn bench_async(c: &mut Criterion) {
    let schema = schema();
    for (name, query) in [("flat", FLAT), ("deep", DEEP), ("fold_recurse", FOLD_RECURSE)] {
        let indexed = parse(&schema, query).unwrap();
        c.bench_function(&format!("async/{name}"), |b| {
            b.iter(|| {
                let stream = interpret_ir_async(
                    Arc::new(SyncToAsyncAdapter::new(Arc::new(NumbersAdapter::new()))),
                    indexed.clone(),
                    Arc::new(BTreeMap::new()),
                )
                .unwrap();
                let rows: Vec<_> =
                    futures_executor::block_on(futures_util::StreamExt::collect::<Vec<_>>(stream));
                let rows = rows.len();
                assert!(rows > 0);
                rows
            })
        });
    }
}

fn bench_error_parity(c: &mut Criterion) {
    let schema = schema();
    let indexed = parse(&schema, DEEP).unwrap();

    c.bench_function("error_parity/raw_adapter", |b| {
        b.iter(|| {
            interpret_ir(
                Arc::new(NumbersAdapter::new()),
                indexed.clone(),
                Arc::new(BTreeMap::new()),
            )
            .unwrap()
            .count()
        })
    });

    c.bench_function("error_parity/infallible", |b| {
        b.iter(|| {
            interpret_ir(
                Arc::new(crate::bench_fallible_twin::InfallibleTwin::new()),
                indexed.clone(),
                Arc::new(BTreeMap::new()),
            )
            .unwrap()
            .count()
        })
    });

    c.bench_function("error_parity/real_error_type", |b| {
        b.iter(|| {
            interpret_ir(
                Arc::new(crate::bench_fallible_twin::FallibleTwin::new()),
                indexed.clone(),
                Arc::new(BTreeMap::new()),
            )
            .unwrap()
            .count()
        })
    });
}

mod bench_fallible_twin {
    use std::sync::Arc;

    use trustfall_core::{
        interpreter::{
            Adapter, AsVertex, ContextIterator, ContextOutcomeIterator, NeighborResolution,
            ResolveEdgeInfo, ResolveInfo, VertexIterator,
        },
        ir::{EdgeParameters, FieldValue},
        numbers_interpreter::{NumbersAdapter, NumbersVertex},
    };

    #[derive(Debug)]
    pub(super) struct RealError;

    impl std::fmt::Display for RealError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("never actually produced")
        }
    }

    impl std::error::Error for RealError {}

    /// Identical structure for both error types: only the error type differs.
    pub(super) struct TwinAdapter<E> {
        inner: NumbersAdapter,
        _error: std::marker::PhantomData<fn() -> E>,
    }

    impl<E> TwinAdapter<E> {
        pub(super) fn new() -> Self {
            Self { inner: NumbersAdapter::new(), _error: std::marker::PhantomData }
        }
    }

    pub(super) type InfallibleTwin = TwinAdapter<std::convert::Infallible>;
    pub(super) type FallibleTwin = TwinAdapter<RealError>;

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
            Box::new(
                self.inner.resolve_property(contexts, type_name, property_name, resolve_info).map(
                    |(ctx, value)| match value {
                        Ok(value) => (ctx, Ok(value)),
                        Err(never) => match never {},
                    },
                ),
            )
        }

        fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
            &self,
            contexts: ContextIterator<'a, V>,
            type_name: &Arc<str>,
            edge_name: &Arc<str>,
            parameters: &EdgeParameters,
            resolve_info: &ResolveEdgeInfo,
        ) -> ContextOutcomeIterator<'a, V, NeighborResolution<'a, Self::Vertex, Self::Error>>
        {
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
}

criterion_group!(benches, bench_sync, bench_async, bench_error_parity);
criterion_main!(benches);
