//! Differential tests for the synchronous and asynchronous execution routes.

use std::{collections::BTreeMap, fs, sync::Arc};

use futures_util::{StreamExt, stream};

use crate::{
    frontend::parse,
    interpreter::{
        Adapter, AsVertex, NeighborResolutionStream, ResolveEdgeInfo, ResolveInfo,
        async_adapter::{AsyncAdapter, ContextOutcomeStream, ContextStream, VertexStream},
        execution::interpret_ir,
    },
    ir::{EdgeParameters, FieldValue},
    numbers_interpreter::NumbersAdapter,
};

use super::engine::interpret_ir as interpret_ir_async;

type Row = BTreeMap<Arc<str>, FieldValue>;

/// Adapts synchronous fixtures for async differential tests.
pub(super) struct SyncToAsyncAdapter<A>(Arc<A>);

impl<A> SyncToAsyncAdapter<A> {
    pub(super) fn new(adapter: Arc<A>) -> Self {
        Self(adapter)
    }
}

impl<'vertex, A> AsyncAdapter<'vertex> for SyncToAsyncAdapter<A>
where
    A: Adapter<'vertex> + 'vertex,
{
    type Vertex = A::Vertex;
    type Error = A::Error;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexStream<'vertex, Result<Self::Vertex, Self::Error>> {
        Box::pin(stream::iter(self.0.resolve_starting_vertices(
            edge_name,
            parameters,
            resolve_info,
        )))
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, Result<FieldValue, Self::Error>> {
        let adapter = Arc::clone(&self.0);
        let type_name = Arc::clone(type_name);
        let property_name = Arc::clone(property_name);
        let resolve_info = resolve_info.clone();
        Box::pin(async_stream::stream! {
            let mut contexts = contexts;
            while let Some(context) = contexts.next().await {
                let outcomes = adapter.resolve_property(
                    Box::new(std::iter::once(context)),
                    &type_name,
                    &property_name,
                    &resolve_info,
                );
                for outcome in outcomes {
                    yield outcome;
                }
            }
        })
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeStream<
        'vertex,
        V,
        NeighborResolutionStream<'vertex, Self::Vertex, Self::Error>,
    > {
        let adapter = Arc::clone(&self.0);
        let type_name = Arc::clone(type_name);
        let edge_name = Arc::clone(edge_name);
        let parameters = parameters.clone();
        let resolve_info = resolve_info.clone();
        Box::pin(async_stream::stream! {
            let mut contexts = contexts;
            while let Some(context) = contexts.next().await {
                let outcomes = adapter.resolve_neighbors(
                    Box::new(std::iter::once(context)),
                    &type_name,
                    &edge_name,
                    &parameters,
                    &resolve_info,
                );
                for (context, resolution) in outcomes {
                    let resolution = resolution.map(|neighbors| {
                        let neighbors: VertexStream<'vertex, Result<Self::Vertex, Self::Error>> =
                            Box::pin(stream::iter(neighbors));
                        neighbors
                    });
                    yield (context, resolution);
                }
            }
        })
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, Result<bool, Self::Error>> {
        let adapter = Arc::clone(&self.0);
        let type_name = Arc::clone(type_name);
        let coerce_to_type = Arc::clone(coerce_to_type);
        let resolve_info = resolve_info.clone();
        Box::pin(async_stream::stream! {
            let mut contexts = contexts;
            while let Some(context) = contexts.next().await {
                let outcomes = adapter.resolve_coercion(
                    Box::new(std::iter::once(context)),
                    &type_name,
                    &coerce_to_type,
                    &resolve_info,
                );
                for outcome in outcomes {
                    yield outcome;
                }
            }
        })
    }
}

fn sync_results(query: &str, arguments: Arc<BTreeMap<Arc<str>, FieldValue>>) -> Vec<Row> {
    let adapter = Arc::new(NumbersAdapter::new());
    let indexed = parse(adapter.schema(), query).expect("query failed to parse");
    interpret_ir(adapter, indexed, arguments)
        .expect("invalid query arguments")
        .map(|row| row.expect("numbers adapter is infallible"))
        .collect()
}

fn async_results(query: &str, arguments: Arc<BTreeMap<Arc<str>, FieldValue>>) -> Vec<Row> {
    let adapter = Arc::new(SyncToAsyncAdapter::new(Arc::new(NumbersAdapter::new())));
    let indexed = parse(adapter.0.schema(), query).expect("query failed to parse");
    let rows = interpret_ir_async(adapter, indexed, arguments).expect("invalid query arguments");
    futures_executor::block_on(
        rows.map(|row| row.expect("numbers adapter is infallible")).collect(),
    )
}

#[test]
fn valid_query_corpus_matches_the_sync_engine() {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data/tests/valid_queries");
    let mut paths: Vec<_> = fs::read_dir(directory)
        .expect("could not read valid query corpus")
        .map(|entry| entry.expect("could not read corpus entry").path())
        .filter(|path| path.to_string_lossy().ends_with(".graphql.ron"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "the valid-query corpus is empty");

    for path in paths {
        let contents = fs::read_to_string(&path).expect("could not read corpus query");
        let test: crate::test_types::TestGraphQLQuery =
            ron::from_str(&contents).expect("could not parse corpus query");
        assert_eq!(test.schema_name, "numbers", "unexpected schema in {path:?}");
        let arguments = Arc::new(
            test.arguments.into_iter().map(|(name, value)| (Arc::from(name), value)).collect(),
        );
        let expected = sync_results(&test.query, Arc::clone(&arguments));
        let actual = async_results(&test.query, arguments);
        assert_eq!(expected, actual, "async results diverged for {path:?}");
    }
}

mod fault_parity {
    use super::{SyncToAsyncAdapter, *};
    use crate::interpreter::{error::ExecutionError, error_propagation_tests as fault};

    fn run_async(
        fault_kind: fault::Fault,
        fail_after: usize,
        query: &str,
    ) -> Vec<Result<Row, ExecutionError<fault::TestError>>> {
        let adapter = Arc::new(fault::FaultyAdapter::new(fault_kind, fail_after));
        let indexed = parse(&adapter.schema(), query).expect("query failed to parse");
        let rows = interpret_ir_async(
            Arc::new(SyncToAsyncAdapter::new(adapter)),
            indexed,
            Arc::new(BTreeMap::new()),
        )
        .expect("invalid query arguments");
        futures_executor::block_on(rows.collect())
    }

    #[test]
    fn failures_match_the_sync_engine() {
        let queries = [
            fault::FLAT,
            fault::SUCCESSOR,
            r#"{ Number(min: 0, max: 50) { successor @optional { ... on Prime { value @output } } } }"#,
            r#"{ Number(min: 1, max: 30) { value @output multiple(max: 10) @fold { factor: value @output } } }"#,
            r#"{ Number(min: 0, max: 8) { successor @recurse(depth: 3) { succ: value @output } } }"#,
        ];
        let faults = [
            fault::Fault::StartingVertices,
            fault::Fault::Property,
            fault::Fault::Neighbors,
            fault::Fault::Coercion,
        ];

        for query in queries {
            for fault in faults {
                for budget in 0..=6 {
                    let expected = fault::run(fault, budget, query);
                    let actual = run_async(fault, budget, query);
                    assert_eq!(expected.len(), actual.len(), "{fault:?}, budget {budget}, {query}");
                    for (expected, actual) in expected.iter().zip(actual.iter()) {
                        assert_eq!(
                            expected.is_ok(),
                            actual.is_ok(),
                            "{fault:?}, budget {budget}, {query}"
                        );
                        if let (Ok(expected), Ok(actual)) = (expected, actual) {
                            assert_eq!(expected, actual, "{fault:?}, budget {budget}, {query}");
                        } else if let (Err(expected), Err(actual)) = (expected, actual) {
                            assert_eq!(
                                format!("{expected:?}"),
                                format!("{actual:?}"),
                                "{fault:?}, budget {budget}, {query}"
                            );
                        }
                    }
                }
            }
        }
    }
}
