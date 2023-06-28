use trustfall::provider::{ContextIterator, ContextOutcomeIterator, EdgeParameters, ResolveEdgeInfo, VertexIterator};

use super::vertex::Vertex;

pub(super) fn resolve_comment_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "byUser" => comment::by_user(contexts, resolve_info),
        "link" => comment::link(contexts, resolve_info),
        "parent" => comment::parent(contexts, resolve_info),
        "reply" => comment::reply(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'Comment'"
            )
        }
    }
}

mod comment {
    use trustfall::provider::{
        ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn by_user<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        todo!("implement edge 'byUser' for type 'Comment'")
    }

    pub(super) fn link<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        todo!("implement edge 'link' for type 'Comment'")
    }

    pub(super) fn parent<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        todo!("implement edge 'parent' for type 'Comment'")
    }

    pub(super) fn reply<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        todo!("implement edge 'reply' for type 'Comment'")
    }
}

pub(super) fn resolve_job_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "link" => job::link(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'Job'"
            )
        }
    }
}

mod job {
    use trustfall::provider::{
        ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn link<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        todo!("implement edge 'link' for type 'Job'")
    }
}

pub(super) fn resolve_story_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "byUser" => story::by_user(contexts, resolve_info),
        "comment" => story::comment(contexts, resolve_info),
        "link" => story::link(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'Story'"
            )
        }
    }
}

mod story {
    use trustfall::provider::{
        ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn by_user<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        todo!("implement edge 'byUser' for type 'Story'")
    }

    pub(super) fn comment<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        todo!("implement edge 'comment' for type 'Story'")
    }

    pub(super) fn link<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        todo!("implement edge 'link' for type 'Story'")
    }
}

pub(super) fn resolve_user_edge<'a>(
    contexts: ContextIterator<'a, Vertex>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
    match edge_name {
        "link" => user::link(contexts, resolve_info),
        "submitted" => user::submitted(contexts, resolve_info),
        _ => {
            unreachable!(
                "attempted to resolve unexpected edge '{edge_name}' on type 'User'"
            )
        }
    }
}

mod user {
    use trustfall::provider::{
        ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn link<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        todo!("implement edge 'link' for type 'User'")
    }

    pub(super) fn submitted<'a>(
        contexts: ContextIterator<'a, Vertex>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
        todo!("implement edge 'submitted' for type 'User'")
    }
}
