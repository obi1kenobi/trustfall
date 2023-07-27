use trustfall::provider::{ContextIterator, ContextOutcomeIterator, EdgeParameters, ResolveEdgeInfo, VertexIterator};

use super::vertex::Vertex;

pub(super) fn resolve_const_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "const" => const_::const_(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'const'"
            )
        }
    }
}

mod const_ {
    use trustfall::provider::{
        resolve_neighbors_with, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo,
        VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn const_<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            |vertex| {
                let vertex = vertex
                    .as_const()
                    .expect("conversion failed, vertex was not a const");
                todo!("get neighbors along edge 'const' for type 'const'")
            },
        )
    }
}

pub(super) fn resolve_continue_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "continue" => continue_::continue_(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'continue'"
            )
        }
    }
}

mod continue_ {
    use trustfall::provider::{
        resolve_neighbors_with, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo,
        VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn continue_<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            |vertex| {
                let vertex = vertex
                    .as_continue()
                    .expect("conversion failed, vertex was not a continue");
                todo!("get neighbors along edge 'continue' for type 'continue'")
            },
        )
    }
}

pub(super) fn resolve_dyn_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "dyn" => dyn_::dyn_(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'dyn'"
            )
        }
    }
}

mod dyn_ {
    use trustfall::provider::{
        resolve_neighbors_with, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo,
        VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn dyn_<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            |vertex| {
                let vertex = vertex
                    .as_dyn()
                    .expect("conversion failed, vertex was not a dyn");
                todo!("get neighbors along edge 'dyn' for type 'dyn'")
            },
        )
    }
}

pub(super) fn resolve_if_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "if" => if_::if_(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'if'"
            )
        }
    }
}

mod if_ {
    use trustfall::provider::{
        resolve_neighbors_with, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo,
        VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn if_<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            |vertex| {
                let vertex = vertex
                    .as_if()
                    .expect("conversion failed, vertex was not a if");
                todo!("get neighbors along edge 'if' for type 'if'")
            },
        )
    }
}

pub(super) fn resolve_mod_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "mod" => mod_::mod_(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'mod'"
            )
        }
    }
}

mod mod_ {
    use trustfall::provider::{
        resolve_neighbors_with, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo,
        VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn mod_<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            |vertex| {
                let vertex = vertex
                    .as_mod()
                    .expect("conversion failed, vertex was not a mod");
                todo!("get neighbors along edge 'mod' for type 'mod'")
            },
        )
    }
}

pub(super) fn resolve_self_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "self" => self_::self_(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'self'"
            )
        }
    }
}

mod self_ {
    use trustfall::provider::{
        resolve_neighbors_with, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo,
        VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn self_<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            |vertex| {
                let vertex = vertex
                    .as_self_()
                    .expect("conversion failed, vertex was not a self");
                todo!("get neighbors along edge 'self' for type 'self'")
            },
        )
    }
}

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
                "attempted to resolve unexpected edge '{edge_name}' on type 'type'"
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
                    .expect("conversion failed, vertex was not a type");
                todo!("get neighbors along edge 'type' for type 'type'")
            },
        )
    }
}

pub(super) fn resolve_unsafe_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "unsafe" => unsafe_::unsafe_(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'unsafe'"
            )
        }
    }
}

mod unsafe_ {
    use trustfall::provider::{
        resolve_neighbors_with, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo,
        VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn unsafe_<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            |vertex| {
                let vertex = vertex
                    .as_unsafe()
                    .expect("conversion failed, vertex was not a unsafe");
                todo!("get neighbors along edge 'unsafe' for type 'unsafe'")
            },
        )
    }
}

pub(super) fn resolve_where_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "where" => where_::where_(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'where'"
            )
        }
    }
}

mod where_ {
    use trustfall::provider::{
        resolve_neighbors_with, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo,
        VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn where_<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            |vertex| {
                let vertex = vertex
                    .as_where()
                    .expect("conversion failed, vertex was not a where");
                todo!("get neighbors along edge 'where' for type 'where'")
            },
        )
    }
}
