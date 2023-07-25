use trustfall::provider::{ContextIterator, ContextOutcomeIterator, EdgeParameters, ResolveEdgeInfo, VertexIterator};

use super::vertex::Vertex;

pub(super) fn resolve_type_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "type" => type_::type_(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'Type'"
            )
        }
    }
}

mod type_ {
    use trustfall::provider::{
        resolve_neighbors_with, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo,
        VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn type_<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            |vertex| {
                let vertex = vertex
                    .as_type()
                    .expect("conversion failed, vertex was not a Type");
                todo!("get neighbors along edge 'type' for type 'Type'")
            },
        )
    }
}
