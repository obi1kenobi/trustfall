use std::{collections::BTreeMap, fmt::Debug, ops::Bound, sync::Arc};

use self::vertex_info::InternalVertexInfo;

use super::InterpretedQuery;
use crate::ir::{
    EdgeKind, EdgeParameters, Eid, FieldValue, IREdge, IRFold, IRQueryComponent, IRVertex, Output,
    Recursive, Vid,
};

mod candidates;
mod dynamic;
mod filters;
mod sealed;
mod vertex_info;

pub use candidates::{CandidateValue, Range};
pub use dynamic::DynamicallyResolvedValue;
pub use vertex_info::{RequiredProperty, VertexInfo};

/// Contains overall information about the query being executed, such as its outputs and variables.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryInfo<'a> {
    query: &'a InterpretedQuery,
}

impl<'a> QueryInfo<'a> {
    fn new(query: &'a InterpretedQuery) -> Self {
        Self { query }
    }

    /// All the names and type information of the output data of this query.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn outputs(&self) -> &BTreeMap<Arc<str>, Output> {
        &self.query.indexed_query.outputs
    }

    /// The variables with which the query was executed.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn variables(&self) -> &Arc<BTreeMap<Arc<str>, FieldValue>> {
        &self.query.arguments
    }
}

/// Enables adapter optimizations by showing how a query uses a vertex. Implements [`VertexInfo`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveInfo {
    query: InterpretedQuery,
    current_vid: Vid,
    vertex_completed: bool,
}

/// Enables adapter optimizations by showing how a query uses a particular edge.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveEdgeInfo {
    query: InterpretedQuery,
    current_vid: Vid,
    target_vid: Vid,
    crossing_eid: Eid,
}

impl ResolveInfo {
    pub(crate) fn new(query: InterpretedQuery, current_vid: Vid, vertex_completed: bool) -> Self {
        Self { query, current_vid, vertex_completed }
    }

    pub(crate) fn into_inner(self) -> InterpretedQuery {
        self.query
    }

    /// Get information about the overall query being executed.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn query(&self) -> QueryInfo<'_> {
        QueryInfo::new(&self.query)
    }
}

impl sealed::__Sealed for ResolveInfo {}
impl sealed::__Sealed for NeighborInfo {}

impl InternalVertexInfo for ResolveInfo {
    #[inline]
    fn query(&self) -> &InterpretedQuery {
        &self.query
    }

    #[inline]
    fn non_binding_filters(&self) -> bool {
        false // We are *at* the vertex itself. Filters always bind here.
    }

    #[inline]
    fn execution_frontier(&self) -> Bound<Vid> {
        if self.vertex_completed {
            Bound::Included(self.current_vid)
        } else {
            // e.g. during type coercions or in `get_starting_vertices()`
            Bound::Excluded(self.current_vid)
        }
    }

    #[inline]
    fn current_vertex(&self) -> &IRVertex {
        &self.current_component().vertices[&self.current_vid]
    }

    #[inline]
    fn current_component(&self) -> &IRQueryComponent {
        // Inside `ResolveInfo`, the starting component and
        // the current component are one and the same.
        self.starting_component()
    }

    #[inline]
    fn starting_component(&self) -> &IRQueryComponent {
        &self.query.indexed_query.vids[&self.current_vid]
    }

    #[inline]
    fn query_variables(&self) -> &BTreeMap<Arc<str>, FieldValue> {
        self.query.arguments.as_ref()
    }

    fn make_non_folded_edge_info(&self, edge: &IREdge) -> EdgeInfo {
        let neighboring_info = NeighborInfo {
            query: self.query.clone(),
            execution_frontier: self.execution_frontier(),
            starting_vertex: self.current_vid,
            neighbor_vertex: edge.to_vid,
            neighbor_path: vec![edge.eid],
            within_optional_scope: edge.optional,
            locally_non_binding_filters: check_locally_non_binding_filters_for_edge(edge),
        };
        EdgeInfo {
            eid: edge.eid,
            parameters: edge.parameters.clone(),
            optional: edge.optional,
            recursive: edge.recursive.clone(),
            folded: false,
            destination: neighboring_info,
        }
    }

    fn make_folded_edge_info(&self, fold: &IRFold) -> EdgeInfo {
        let at_least_one_element_required =
            filters::fold_requires_at_least_one_element(&self.query.arguments, fold);

        let neighboring_info = NeighborInfo {
            query: self.query.clone(),
            execution_frontier: self.execution_frontier(),
            starting_vertex: self.current_vid,
            neighbor_vertex: fold.to_vid,
            neighbor_path: vec![fold.eid],
            within_optional_scope: !at_least_one_element_required,
            locally_non_binding_filters: false,
        };
        EdgeInfo {
            eid: fold.eid,
            parameters: fold.parameters.clone(),
            optional: false,
            recursive: None,
            folded: true,
            destination: neighboring_info,
        }
    }
}

impl ResolveEdgeInfo {
    pub(crate) fn new(
        query: InterpretedQuery,
        current_vid: Vid,
        target_vid: Vid,
        crossing_eid: Eid,
    ) -> Self {
        Self { query, current_vid, target_vid, crossing_eid }
    }

    pub(crate) fn into_inner(self) -> InterpretedQuery {
        self.query
    }

    /// Get information about the overall query being executed.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn query(&self) -> QueryInfo<'_> {
        QueryInfo::new(&self.query)
    }

    /// The unique ID of this edge within its query.
    #[inline]
    pub fn eid(&self) -> Eid {
        self.crossing_eid
    }

    /// The unique ID of the vertex where the edge in this operation begins.
    #[inline]
    pub fn origin_vid(&self) -> Vid {
        self.current_vid
    }

    /// The unique ID of the vertex to which this edge points.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn destination_vid(&self) -> Vid {
        self.target_vid
    }

    /// Info about the destination vertex of the edge being expanded where this value was provided.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn destination(&self) -> NeighborInfo {
        self.edge().destination
    }

    /// Info about the edge being expanded where this value was provided.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn edge(&self) -> EdgeInfo {
        let eid = self.eid();
        match &self.query.indexed_query.eids[&eid] {
            EdgeKind::Regular(regular) => {
                debug_assert_eq!(
                    self.target_vid, regular.to_vid,
                    "expected Vid {:?} but got {:?} in edge {regular:?}",
                    self.target_vid, regular.to_vid
                );
                EdgeInfo {
                    eid,
                    parameters: regular.parameters.clone(),
                    optional: regular.optional,
                    recursive: regular.recursive.clone(),
                    folded: false,
                    destination: NeighborInfo {
                        query: self.query.clone(),
                        execution_frontier: Bound::Excluded(self.target_vid),
                        starting_vertex: self.origin_vid(),
                        neighbor_vertex: regular.to_vid,
                        neighbor_path: vec![eid],
                        within_optional_scope: regular.optional,
                        locally_non_binding_filters: check_locally_non_binding_filters_for_edge(
                            regular,
                        ),
                    },
                }
            }
            EdgeKind::Fold(fold) => {
                debug_assert_eq!(
                    self.target_vid, fold.to_vid,
                    "expected Vid {:?} but got {:?} in fold {fold:?}",
                    self.target_vid, fold.to_vid
                );

                // In this case, we are *currently* resolving the folded edge.
                // Its own filters always apply, and we don't need to check whether
                // the fold is required to have at least one element.
                let within_optional_scope = false;

                EdgeInfo {
                    eid,
                    parameters: fold.parameters.clone(),
                    optional: false,
                    recursive: None,
                    folded: true,
                    destination: NeighborInfo {
                        query: self.query.clone(),
                        execution_frontier: Bound::Excluded(self.target_vid),
                        starting_vertex: self.origin_vid(),
                        neighbor_vertex: fold.to_vid,
                        neighbor_path: vec![eid],
                        within_optional_scope,
                        locally_non_binding_filters: false,
                    },
                }
            }
        }
    }
}

/// Information about an edge that is being resolved as part of a query.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeInfo {
    eid: Eid,
    parameters: EdgeParameters,
    optional: bool,
    recursive: Option<Recursive>,
    folded: bool,
    destination: NeighborInfo,
}

impl EdgeInfo {
    /// The ID that uniquely identifies this edge in its query.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn eid(&self) -> Eid {
        self.eid
    }

    /// The values with which this edge was parameterized.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn parameters(&self) -> &EdgeParameters {
        &self.parameters
    }

    /// Info about the vertex to which this edge points.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn destination(&self) -> &NeighborInfo {
        &self.destination
    }

    /// Whether this edge is required to exist, or else the computed row will be discarded.
    #[inline]
    pub fn is_mandatory(&self) -> bool {
        !self.folded && !self.optional && self.recursive.is_none()
    }
}

/// Information about a neighboring vertex. Implements [`VertexInfo`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeighborInfo {
    query: InterpretedQuery,
    execution_frontier: Bound<Vid>, // how far query execution has progressed
    starting_vertex: Vid,
    neighbor_vertex: Vid,
    neighbor_path: Vec<Eid>,

    // Filtering operations (filters, required edges, etc.) don't bind inside an `@optional` scope,
    // to ensure that optional-then-filter semantics are upheld and avoid adapter footguns.
    // They also don't bind inside a `@fold` scope that is not mandated to have >= 1 element,
    // since such scopes also have optional semantics: no matter what property values are
    // within such a scope, they cannot enable an earlier edge resolver to early-discard vertices.
    //
    // For specific examples, see test cases with names like:
    // - `optional_with_nested_filter_*`
    // - `fold_with_nested_filter`
    // - `fold_with_nested_filter_and_tag`
    // - `fold_with_count_filter_and_nested_filter`
    // - `fold_with_count_filter_and_nested_filter_with_tag`
    // - `fold_with_both_count_and_nested_filter_dependent_on_tag`
    within_optional_scope: bool,

    // A situation specific to *this vertex in particular* makes filters non-binding.
    // For example: a `@recurse(depth: 2)` edge.
    locally_non_binding_filters: bool,
}

impl InternalVertexInfo for NeighborInfo {
    #[inline]
    fn query(&self) -> &InterpretedQuery {
        &self.query
    }

    #[inline]
    fn non_binding_filters(&self) -> bool {
        self.within_optional_scope || self.locally_non_binding_filters
    }

    #[inline]
    fn execution_frontier(&self) -> Bound<Vid> {
        self.execution_frontier
    }

    #[inline]
    fn current_vertex(&self) -> &IRVertex {
        &self.current_component().vertices[&self.neighbor_vertex]
    }

    #[inline]
    fn current_component(&self) -> &IRQueryComponent {
        &self.query.indexed_query.vids[&self.neighbor_vertex]
    }

    #[inline]
    fn starting_component(&self) -> &IRQueryComponent {
        &self.query.indexed_query.vids[&self.starting_vertex]
    }

    #[inline]
    fn query_variables(&self) -> &BTreeMap<Arc<str>, FieldValue> {
        self.query.arguments.as_ref()
    }

    fn make_non_folded_edge_info(&self, edge: &IREdge) -> EdgeInfo {
        let mut neighbor_path = self.neighbor_path.clone();
        neighbor_path.push(edge.eid);

        let neighboring_info = NeighborInfo {
            query: self.query.clone(),
            execution_frontier: self.execution_frontier,
            starting_vertex: self.starting_vertex,
            neighbor_vertex: edge.to_vid,
            neighbor_path,
            within_optional_scope: self.within_optional_scope,
            locally_non_binding_filters: check_locally_non_binding_filters_for_edge(edge),
        };
        EdgeInfo {
            eid: edge.eid,
            parameters: edge.parameters.clone(),
            optional: edge.optional,
            recursive: edge.recursive.clone(),
            folded: false,
            destination: neighboring_info,
        }
    }

    fn make_folded_edge_info(&self, fold: &IRFold) -> EdgeInfo {
        let at_least_one_element_required =
            filters::fold_requires_at_least_one_element(&self.query.arguments, fold);

        let mut neighbor_path = self.neighbor_path.clone();
        neighbor_path.push(fold.eid);
        let neighboring_info = NeighborInfo {
            query: self.query.clone(),
            execution_frontier: self.execution_frontier,
            starting_vertex: self.starting_vertex,
            neighbor_vertex: fold.to_vid,
            neighbor_path,
            within_optional_scope: self.within_optional_scope || !at_least_one_element_required,
            locally_non_binding_filters: false,
        };
        EdgeInfo {
            eid: fold.eid,
            parameters: fold.parameters.clone(),
            optional: false,
            recursive: None,
            folded: true,
            destination: neighboring_info,
        }
    }
}

/// For recursive edges to depth 2+, filter operations at the destination
/// do not affect whether the edge is taken or not.
///
/// The query semantics state that recursive edge traversals happen first, then filters are applied.
/// At depth 1, those filters are applied after only one edge expansion, so filters "count".
/// With recursions to depth 2+, there are "middle" layers of vertices that don't have to satisfy
/// the filters (and will be filtered out) but can still have more edge expansions in the recursion.
fn check_locally_non_binding_filters_for_edge(edge: &IREdge) -> bool {
    edge.recursive.as_ref().map(|r| r.depth.get() >= 2).unwrap_or(false)
}

#[cfg(test)]
mod tests;
