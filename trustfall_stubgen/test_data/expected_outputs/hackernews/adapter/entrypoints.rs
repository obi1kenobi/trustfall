use trustfall::provider::{ResolveInfo, VertexIterator};

use super::vertex::Vertex;

pub(super) fn front_page<'a>(_resolve_info: &ResolveInfo) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'FrontPage'")
}

pub(super) fn top<'a>(
    max: Option<i64>,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'Top'")
}

pub(super) fn latest<'a>(
    max: Option<i64>,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'Latest'")
}

pub(super) fn best<'a>(
    max: Option<i64>,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'Best'")
}

pub(super) fn ask_hn<'a>(
    max: Option<i64>,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'AskHN'")
}

pub(super) fn show_hn<'a>(
    max: Option<i64>,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'ShowHN'")
}

pub(super) fn recent_job<'a>(
    max: Option<i64>,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'RecentJob'")
}

pub(super) fn user<'a>(
    name: &str,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'User'")
}

pub(super) fn item<'a>(
    id: i64,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'Item'")
}

pub(super) fn updated_item<'a>(
    max: Option<i64>,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'UpdatedItem'")
}

pub(super) fn updated_user_profile<'a>(
    max: Option<i64>,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!(
        "implement resolving starting vertices for entrypoint edge 'UpdatedUserProfile'"
    )
}

pub(super) fn search_by_relevance<'a>(
    query: &str,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!(
        "implement resolving starting vertices for entrypoint edge 'SearchByRelevance'"
    )
}

pub(super) fn search_by_date<'a>(
    query: &str,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'SearchByDate'")
}
