use trustfall::provider::{AsVertex, ContextIterator, ContextOutcomeIterator, EdgeParameters, ResolveEdgeInfo, VertexIterator};

use super::vertex::Vertex;

pub(super) fn resolve_comment_edge<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
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
        resolve_neighbors_with, AsVertex, ContextIterator, ContextOutcomeIterator,
        ResolveEdgeInfo, VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn by_user<'a, V: AsVertex<Vertex> + 'a>(
        contexts: ContextIterator<'a, V>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            move |vertex| {
                let vertex = vertex
                    .as_comment()
                    .expect("conversion failed, vertex was not a Comment");
                todo!("get neighbors along edge 'byUser' for type 'Comment'")
            },
        )
    }

    pub(super) fn link<'a, V: AsVertex<Vertex> + 'a>(
        contexts: ContextIterator<'a, V>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            move |vertex| {
                let vertex = vertex
                    .as_comment()
                    .expect("conversion failed, vertex was not a Comment");
                todo!("get neighbors along edge 'link' for type 'Comment'")
            },
        )
    }

    pub(super) fn parent<'a, V: AsVertex<Vertex> + 'a>(
        contexts: ContextIterator<'a, V>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            move |vertex| {
                let vertex = vertex
                    .as_comment()
                    .expect("conversion failed, vertex was not a Comment");
                todo!("get neighbors along edge 'parent' for type 'Comment'")
            },
        )
    }

    pub(super) fn reply<'a, V: AsVertex<Vertex> + 'a>(
        contexts: ContextIterator<'a, V>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            move |vertex| {
                let vertex = vertex
                    .as_comment()
                    .expect("conversion failed, vertex was not a Comment");
                todo!("get neighbors along edge 'reply' for type 'Comment'")
            },
        )
    }
}

pub(super) fn resolve_job_edge<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
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
        resolve_neighbors_with, AsVertex, ContextIterator, ContextOutcomeIterator,
        ResolveEdgeInfo, VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn link<'a, V: AsVertex<Vertex> + 'a>(
        contexts: ContextIterator<'a, V>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            move |vertex| {
                let vertex = vertex
                    .as_job()
                    .expect("conversion failed, vertex was not a Job");
                todo!("get neighbors along edge 'link' for type 'Job'")
            },
        )
    }
}

pub(super) fn resolve_story_edge<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
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
        resolve_neighbors_with, AsVertex, ContextIterator, ContextOutcomeIterator,
        ResolveEdgeInfo, VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn by_user<'a, V: AsVertex<Vertex> + 'a>(
        contexts: ContextIterator<'a, V>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            move |vertex| {
                let vertex = vertex
                    .as_story()
                    .expect("conversion failed, vertex was not a Story");
                todo!("get neighbors along edge 'byUser' for type 'Story'")
            },
        )
    }

    pub(super) fn comment<'a, V: AsVertex<Vertex> + 'a>(
        contexts: ContextIterator<'a, V>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            move |vertex| {
                let vertex = vertex
                    .as_story()
                    .expect("conversion failed, vertex was not a Story");
                todo!("get neighbors along edge 'comment' for type 'Story'")
            },
        )
    }

    pub(super) fn link<'a, V: AsVertex<Vertex> + 'a>(
        contexts: ContextIterator<'a, V>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            move |vertex| {
                let vertex = vertex
                    .as_story()
                    .expect("conversion failed, vertex was not a Story");
                todo!("get neighbors along edge 'link' for type 'Story'")
            },
        )
    }
}

pub(super) fn resolve_user_edge<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    edge_name: &str,
    parameters: &EdgeParameters,
    resolve_info: &ResolveEdgeInfo,
) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
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
        resolve_neighbors_with, AsVertex, ContextIterator, ContextOutcomeIterator,
        ResolveEdgeInfo, VertexIterator,
    };

    use super::super::vertex::Vertex;

    pub(super) fn link<'a, V: AsVertex<Vertex> + 'a>(
        contexts: ContextIterator<'a, V>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            move |vertex| {
                let vertex = vertex
                    .as_user()
                    .expect("conversion failed, vertex was not a User");
                todo!("get neighbors along edge 'link' for type 'User'")
            },
        )
    }

    pub(super) fn submitted<'a, V: AsVertex<Vertex> + 'a>(
        contexts: ContextIterator<'a, V>,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Vertex>> {
        resolve_neighbors_with(
            contexts,
            move |vertex| {
                let vertex = vertex
                    .as_user()
                    .expect("conversion failed, vertex was not a User");
                todo!("get neighbors along edge 'submitted' for type 'User'")
            },
        )
    }
}
