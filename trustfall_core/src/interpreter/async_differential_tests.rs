//! Differential tests for the synchronous and asynchronous execution routes.

use std::{collections::BTreeMap, fs, sync::Arc};

use futures_util::{StreamExt, stream};

use crate::{
    frontend::parse,
    interpreter::{
        AsVertex, FallibleAdapter, NeighborOutcomeStream, ResolveEdgeInfo, ResolveInfo,
        async_adapter::{ContextOutcomeStream, ContextStream, FallibleAsyncAdapter, VertexStream},
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

impl<'vertex, A> FallibleAsyncAdapter<'vertex> for SyncToAsyncAdapter<A>
where
    A: FallibleAdapter<'vertex> + 'vertex,
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
        contexts: ContextStream<'vertex, V, Self::Error>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, FieldValue, Self::Error> {
        let adapter = Arc::clone(&self.0);
        let type_name = Arc::clone(type_name);
        let property_name = Arc::clone(property_name);
        let resolve_info = resolve_info.clone();
        Box::pin(async_stream::stream! {
            let mut contexts = contexts;
            while let Some(result) = contexts.next().await {
                let context = match result {
                    Ok(context) => context,
                    Err(error) => {
                        yield Err(error);
                        continue;
                    }
                };
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
        contexts: ContextStream<'vertex, V, Self::Error>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeStream<
        'vertex,
        V,
        NeighborOutcomeStream<'vertex, Self::Vertex, Self::Error>,
        Self::Error,
    > {
        let adapter = Arc::clone(&self.0);
        let type_name = Arc::clone(type_name);
        let edge_name = Arc::clone(edge_name);
        let parameters = parameters.clone();
        let resolve_info = resolve_info.clone();
        Box::pin(async_stream::stream! {
            let mut contexts = contexts;
            while let Some(result) = contexts.next().await {
                let context = match result {
                    Ok(context) => context,
                    Err(error) => {
                        yield Err(error);
                        continue;
                    }
                };
                let outcomes = adapter.resolve_neighbors(
                    Box::new(std::iter::once(context)),
                    &type_name,
                    &edge_name,
                    &parameters,
                    &resolve_info,
                );
                for outcome in outcomes {
                    yield outcome.map(|(context, neighbors)| {
                        let neighbors: VertexStream<
                            'vertex,
                            Result<Self::Vertex, Self::Error>,
                        > = Box::pin(stream::iter(neighbors));
                        (context, neighbors)
                    });
                }
            }
        })
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V, Self::Error>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, bool, Self::Error> {
        let adapter = Arc::clone(&self.0);
        let type_name = Arc::clone(type_name);
        let coerce_to_type = Arc::clone(coerce_to_type);
        let resolve_info = resolve_info.clone();
        Box::pin(async_stream::stream! {
            let mut contexts = contexts;
            while let Some(result) = contexts.next().await {
                let context = match result {
                    Ok(context) => context,
                    Err(error) => {
                        yield Err(error);
                        continue;
                    }
                };
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
