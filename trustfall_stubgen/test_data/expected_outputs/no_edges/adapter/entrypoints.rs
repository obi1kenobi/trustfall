use trustfall::provider::{ResolveInfo, VertexIterator};

use super::vertex::Vertex;

pub(super) fn front_page<'a>(_resolve_info: &ResolveInfo) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'FrontPage'")
}

pub(super) fn item<'a>(
    id: i64,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'Item'")
}

pub(super) fn search_by_date<'a>(
    query: &str,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'SearchByDate'")
}

pub(super) fn search_by_relevance<'a>(
    query: &str,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!(
        "implement resolving starting vertices for entrypoint edge 'SearchByRelevance'"
    )
}

pub(super) fn top<'a>(
    max: Option<i64>,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'Top'")
}

pub(super) fn updated_item<'a>(
    max: Option<i64>,
    _resolve_info: &ResolveInfo,
) -> VertexIterator<'a, Vertex> {
    todo!("implement resolving starting vertices for entrypoint edge 'UpdatedItem'")
}
