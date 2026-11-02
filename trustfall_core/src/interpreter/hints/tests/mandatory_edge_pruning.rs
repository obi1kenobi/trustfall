use std::{collections::BTreeMap, sync::Arc};

use maplit::btreemap;

use crate::{
    interpreter::{
        Adapter, AsVertex, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, ResolveInfo,
        VertexInfo, VertexIterator, execution::interpret_ir,
    },
    ir::{EdgeParameters, FieldValue},
    numbers_interpreter::{NumbersAdapter, NumbersVertex},
};

/// Uses mandatory-edge hints to skip neighbors that cannot have the requested multiples.
#[derive(Default)]
struct PruneByMandatoryMultiple {
    inner: NumbersAdapter,
}

impl<'a> Adapter<'a> for PruneByMandatoryMultiple {
    type Vertex = NumbersVertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexIterator<'a, Self::Vertex> {
        self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info)
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, FieldValue> {
        self.inner.resolve_property(contexts, type_name, property_name, resolve_info)
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Self::Vertex>> {
        let mandatory_multiple_max = resolve_info
            .destination()
            .first_edge("multiple")
            .filter(|edge| edge.is_mandatory())
            .map(|edge| edge.parameters()["max"].as_i64().unwrap());
        let resolved =
            self.inner.resolve_neighbors(contexts, type_name, edge_name, parameters, resolve_info);

        Box::new(resolved.map(move |(context, neighbors)| {
            // NumbersAdapter's `multiple` edge returns only composite numbers:
            // zero and one have no neighbors, primes start at twice their value,
            // and composites start with themselves.
            let neighbors = neighbors.filter(move |vertex| match mandatory_multiple_max {
                None => true,
                Some(max) => match vertex {
                    NumbersVertex::Neither(_) => false,
                    NumbersVertex::Prime(_) => max >= 2,
                    NumbersVertex::Composite(_) => max >= 1,
                    NumbersVertex::Letter(_) => unreachable!("multiple is an edge on numbers"),
                },
            });
            (context, Box::new(neighbors) as VertexIterator<'a, Self::Vertex>)
        }))
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, bool> {
        self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
    }
}

#[test]
fn pruning_preserves_present_but_nonmatching_optional_neighbors() {
    let query = r#"
{
    One {
        predecessor @optional {
            multiple(max: 2) {
                value @output
            }
        }
    }
}"#;
    let adapter = Arc::new(PruneByMandatoryMultiple::default());
    let query = crate::frontend::parse(adapter.inner.schema(), query).unwrap();
    let rows = interpret_ir(adapter, query, Arc::new(BTreeMap::new())).unwrap().collect::<Vec<_>>();

    // Zero exists, but has no multiples. Pruning it would make the optional edge
    // appear absent and incorrectly preserve a row with a null output.
    assert!(rows.is_empty(), "{rows:?}");
}

#[test]
fn pruning_preserves_recursive_intermediate_vertices() {
    let query = r#"
{
    Zero {
        successor @recurse(depth: 2) {
            multiple(max: 2) {
                value @output
            }
        }
    }
}"#;
    let adapter = Arc::new(PruneByMandatoryMultiple::default());
    let query = crate::frontend::parse(adapter.inner.schema(), query).unwrap();
    let rows = interpret_ir(adapter, query, Arc::new(BTreeMap::new())).unwrap().collect::<Vec<_>>();

    // One has no multiples, but must remain traversable so recursion reaches two.
    assert_eq!(rows, vec![btreemap! { Arc::from("value") => FieldValue::Int64(4) }]);
}
