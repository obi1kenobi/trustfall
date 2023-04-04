use std::{collections::BTreeMap, fmt::Debug, ops::Bound, sync::Arc};

use self::vertex_info::InternalVertexInfo;

use super::InterpretedQuery;
use crate::ir::{
    EdgeKind, EdgeParameters, Eid, FieldValue, IREdge, IRFold, IRQueryComponent, IRVertex, Output,
    Recursive, Vid,
};

mod candidates;
mod dynamic;
mod sealed;
mod vertex_info;

pub use candidates::{CandidateValue, Range};
pub use dynamic::DynamicallyResolvedValue;
pub use vertex_info::VertexInfo;

/// Information about the query being processed.
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

    /// The arguments with which the query was executed.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn arguments(&self) -> &Arc<BTreeMap<Arc<str>, FieldValue>> {
        &self.query.arguments
    }
}

/// Information about how vertex data is being resolved.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveInfo {
    query: InterpretedQuery,
    current_vid: Vid,
    vertex_completed: bool,
}

/// Information about an edge is being resolved.
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
        Self {
            query,
            current_vid,
            vertex_completed,
        }
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
            non_binding_filters: check_non_binding_filters_for_edge(edge),
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
        let neighboring_info = NeighborInfo {
            query: self.query.clone(),
            execution_frontier: self.execution_frontier(),
            starting_vertex: self.current_vid,
            neighbor_vertex: fold.to_vid,
            neighbor_path: vec![fold.eid],
            non_binding_filters: false,
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
        Self {
            query,
            current_vid,
            target_vid,
            crossing_eid,
        }
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
                        non_binding_filters: check_non_binding_filters_for_edge(regular),
                    },
                }
            }
            EdgeKind::Fold(fold) => {
                debug_assert_eq!(
                    self.target_vid, fold.to_vid,
                    "expected Vid {:?} but got {:?} in fold {fold:?}",
                    self.target_vid, fold.to_vid
                );
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
                        non_binding_filters: false,
                    },
                }
            }
        }
    }
}

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
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeighborInfo {
    query: InterpretedQuery,
    execution_frontier: Bound<Vid>, // how far query execution has progressed
    starting_vertex: Vid,
    neighbor_vertex: Vid,
    neighbor_path: Vec<Eid>,
    non_binding_filters: bool, // for example, inside a `@recurse(depth: 2)` edge
}

impl InternalVertexInfo for NeighborInfo {
    #[inline]
    fn query(&self) -> &InterpretedQuery {
        &self.query
    }

    #[inline]
    fn non_binding_filters(&self) -> bool {
        self.non_binding_filters
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
            non_binding_filters: check_non_binding_filters_for_edge(edge),
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
        let mut neighbor_path = self.neighbor_path.clone();
        neighbor_path.push(fold.eid);
        let neighboring_info = NeighborInfo {
            query: self.query.clone(),
            execution_frontier: self.execution_frontier,
            starting_vertex: self.starting_vertex,
            neighbor_vertex: fold.to_vid,
            neighbor_path,
            non_binding_filters: false,
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
fn check_non_binding_filters_for_edge(edge: &IREdge) -> bool {
    edge.recursive
        .as_ref()
        .map(|r| r.depth.get() >= 2)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell, collections::BTreeMap, num::NonZeroUsize, path::PathBuf, rc::Rc, sync::Arc,
    };

    use super::{ResolveEdgeInfo, ResolveInfo};
    use crate::{
        interpreter::{
            execution::interpret_ir, Adapter, ContextIterator, ContextOutcomeIterator, VertexInfo,
            VertexIterator,
        },
        ir::{Eid, FieldValue, Recursive, Vid},
        numbers_interpreter::NumbersAdapter,
        util::{TestIRQuery, TestIRQueryResult},
    };

    type ResolveInfoFn = Box<dyn FnMut(&ResolveInfo)>;
    type ResolveEdgeInfoFn = Box<dyn FnMut(&ResolveEdgeInfo)>;

    #[derive(Default)]
    struct TrackCalls<F> {
        underlying: Option<F>,
        calls: usize,
    }

    impl<F> TrackCalls<F> {
        fn new_underlying(underlying: F) -> Self {
            Self {
                underlying: Some(underlying),
                calls: 0,
            }
        }
    }

    impl TrackCalls<ResolveInfoFn> {
        fn call(&mut self, info: &ResolveInfo) {
            self.calls += 1;
            if let Some(underlying) = self.underlying.as_mut() {
                underlying(info);
            }
        }
    }

    impl TrackCalls<ResolveEdgeInfoFn> {
        fn call(&mut self, info: &ResolveEdgeInfo) {
            self.calls += 1;
            if let Some(underlying) = self.underlying.as_mut() {
                underlying(info);
            }
        }
    }

    struct TestAdapter {
        on_starting_vertices: BTreeMap<Vid, TrackCalls<ResolveInfoFn>>,
        on_property_resolver: BTreeMap<Vid, TrackCalls<ResolveInfoFn>>,
        on_edge_resolver: BTreeMap<Eid, TrackCalls<ResolveEdgeInfoFn>>,
        on_type_coercion: BTreeMap<Vid, TrackCalls<ResolveInfoFn>>,
        inner: NumbersAdapter,
    }

    impl TestAdapter {
        fn new() -> Self {
            Self {
                inner: NumbersAdapter::new(),
                on_starting_vertices: Default::default(),
                on_property_resolver: Default::default(),
                on_edge_resolver: Default::default(),
                on_type_coercion: Default::default(),
            }
        }
    }

    impl Default for TestAdapter {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Adapter<'static> for TestAdapter {
        type Vertex = <NumbersAdapter as Adapter<'static>>::Vertex;

        fn resolve_starting_vertices(
            &mut self,
            edge_name: &Arc<str>,
            parameters: &crate::ir::EdgeParameters,
            resolve_info: &super::ResolveInfo,
        ) -> VertexIterator<'static, Self::Vertex> {
            if let Some(x) = self.on_starting_vertices.get_mut(&resolve_info.current_vid) {
                x.call(resolve_info);
            }
            self.inner
                .resolve_starting_vertices(edge_name, parameters, resolve_info)
        }

        fn resolve_property(
            &mut self,
            contexts: ContextIterator<'static, Self::Vertex>,
            type_name: &Arc<str>,
            property_name: &Arc<str>,
            resolve_info: &super::ResolveInfo,
        ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
            if let Some(x) = self.on_property_resolver.get_mut(&resolve_info.current_vid) {
                x.call(resolve_info);
            }
            self.inner
                .resolve_property(contexts, type_name, property_name, resolve_info)
        }

        fn resolve_neighbors(
            &mut self,
            contexts: ContextIterator<'static, Self::Vertex>,
            type_name: &Arc<str>,
            edge_name: &Arc<str>,
            parameters: &crate::ir::EdgeParameters,
            resolve_info: &super::ResolveEdgeInfo,
        ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>>
        {
            if let Some(x) = self.on_edge_resolver.get_mut(&resolve_info.eid()) {
                x.call(resolve_info);
            }
            self.inner
                .resolve_neighbors(contexts, type_name, edge_name, parameters, resolve_info)
        }

        fn resolve_coercion(
            &mut self,
            contexts: ContextIterator<'static, Self::Vertex>,
            type_name: &Arc<str>,
            coerce_to_type: &Arc<str>,
            resolve_info: &super::ResolveInfo,
        ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
            if let Some(x) = self.on_type_coercion.get_mut(&resolve_info.current_vid) {
                x.call(resolve_info);
            }
            self.inner
                .resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
        }
    }

    fn get_ir_for_named_query(stem: &str) -> TestIRQuery {
        let mut input_path = PathBuf::from("test_data/tests/valid_queries");
        input_path.push(format!("{stem}.ir.ron"));
        let input_data = std::fs::read_to_string(input_path).unwrap();
        let test_query: TestIRQueryResult = ron::from_str(&input_data).unwrap();
        test_query.unwrap()
    }

    fn vid(n: usize) -> Vid {
        Vid::new(n.try_into().unwrap())
    }

    fn eid(n: usize) -> Eid {
        Eid::new(n.try_into().unwrap())
    }

    fn run_query<A: Adapter<'static> + 'static>(adapter: A, input_name: &str) -> Rc<RefCell<A>> {
        let input = get_ir_for_named_query(input_name);
        let adapter = Rc::from(RefCell::new(adapter));
        let _ = interpret_ir(
            adapter.clone(),
            Arc::new(input.ir_query.try_into().unwrap()),
            Arc::new(
                input
                    .arguments
                    .iter()
                    .map(|(k, v)| (Arc::from(k.to_owned()), v.clone()))
                    .collect(),
            ),
        )
        .unwrap()
        .collect::<Vec<_>>();
        adapter
    }

    #[test]
    fn root_coercion() {
        let input_name = "root_coercion";

        let adapter = TestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(info.coerced_to_type().map(|x| x.as_ref()), Some("Prime"));
                    assert_eq!(info.vid(), vid(1));
                })),
            },
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.borrow().on_starting_vertices[&vid(1)].calls, 1);
    }

    #[test]
    fn duplicated_edge() {
        let input_name = "duplicated_edge";

        let adapter = TestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert!(info.coerced_to_type().is_none());
                    assert_eq!(info.vid(), vid(1));

                    let edges: Vec<_> = info.edges_with_name("successor").collect();
                    assert_eq!(edges.len(), 2);

                    let first_edge = &edges[0];
                    let second_edge = &edges[1];
                    assert_eq!(first_edge.eid(), eid(1));
                    assert!(first_edge.parameters().is_empty());
                    assert_eq!(second_edge.eid(), eid(2));
                    assert!(second_edge.parameters().is_empty());

                    // The "first_*" methods produce the same outcome as the iterator-based method.
                    assert_eq!(first_edge, &info.first_edge("successor").unwrap());
                    assert_eq!(first_edge, &info.first_mandatory_edge("successor").unwrap());

                    let first_destination = first_edge.destination();
                    let second_destination = second_edge.destination();

                    assert_eq!(first_destination.vid(), vid(2));
                    assert_eq!(second_destination.vid(), vid(3));
                })),
            },
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(info.origin_vid(), vid(1));

                    let destination = info.destination();
                    assert_eq!(destination.vid(), vid(2));
                })),
                eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(info.origin_vid(), vid(1));

                    let destination = info.destination();
                    assert_eq!(destination.vid(), vid(3));
                })),
            },
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        let adapter_ref = adapter.borrow();
        assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
        assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 1);
        assert_eq!(adapter_ref.on_edge_resolver[&eid(2)].calls, 1);
    }

    #[test]
    fn optional_directive() {
        let input_name = "optional_directive";

        let adapter = TestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert!(info.coerced_to_type().is_none());
                    assert_eq!(info.vid(), vid(1));

                    let edges: Vec<_> = info.edges_with_name("multiple").collect();
                    assert_eq!(edges.len(), 1);

                    let edge = &edges[0];
                    assert_eq!(edge.eid(), eid(1));
                    assert_eq!(edge.parameters().get("max"), Some(&(3i64.into())));
                    assert!(edge.optional);
                    assert!(!edge.folded);
                    assert!(edge.recursive.is_none());
                    assert_eq!(edge.destination().vid(), vid(2));

                    // The "first_edge()" method produces the same outcome
                    // `as the iterator-based method.
                    assert_eq!(edge, &info.first_edge("multiple").expect("no edge returned"));

                    // This edge is not mandatory, so it isn't returned.
                    assert!(info.first_mandatory_edge("multiple").is_none())
                })),
            },
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(info.origin_vid(), vid(1));

                    let destination = info.destination();
                    assert_eq!(destination.vid(), vid(2));
                })),
            },
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        let adapter_ref = adapter.borrow();
        assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
        assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 1);
    }

    #[test]
    fn recurse_directive() {
        let input_name = "recurse_directive";

        let adapter = TestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert!(info.coerced_to_type().is_none());
                    assert_eq!(info.vid(), vid(1));

                    let edges: Vec<_> = info.edges_with_name("successor").collect();
                    assert_eq!(edges.len(), 1);

                    let edge = &edges[0];
                    assert_eq!(edge.eid(), eid(1));
                    assert!(edge.parameters().is_empty());
                    assert!(!edge.optional);
                    assert!(!edge.folded);
                    assert_eq!(edge.recursive, Some(Recursive::new(NonZeroUsize::new(3).unwrap(), None)));
                    assert_eq!(edge.destination().vid(), vid(2));

                    // The "first_edge()" method produces the same outcome
                    // `as the iterator-based method.
                    assert_eq!(edge, &info.first_edge("successor").expect("no edge returned"));

                    // This edge is not mandatory, so it isn't returned.
                    assert!(info.first_mandatory_edge("successor").is_none())
                })),
            },
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(info.origin_vid(), vid(1));

                    let destination = info.destination();
                    assert_eq!(destination.vid(), vid(2));
                })),
            },
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        let adapter_ref = adapter.borrow();
        assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
        assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 3); // depth 3 recursion
    }

    #[test]
    fn fold_directive() {
        let input_name = "fold_directive";

        let adapter = TestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert!(info.coerced_to_type().is_none());
                    assert_eq!(info.vid(), vid(1));

                    let edges: Vec<_> = info.edges_with_name("multiple").collect();
                    assert_eq!(edges.len(), 1);

                    let edge = &edges[0];
                    assert_eq!(edge.eid(), eid(1));
                    assert_eq!(edge.parameters().get("max"), Some(&(3i64.into())));
                    assert!(!edge.optional);
                    assert!(edge.folded);
                    assert!(edge.recursive.is_none());
                    assert_eq!(edge.destination().vid(), vid(2));

                    // The "first_edge()" method produces the same outcome
                    // `as the iterator-based method.
                    assert_eq!(edge, &info.first_edge("multiple").expect("no edge returned"));

                    // This edge is not mandatory, so it isn't returned.
                    assert!(info.first_mandatory_edge("multiple").is_none())
                })),
            },
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(info.origin_vid(), vid(1));

                    let destination = info.destination();
                    assert_eq!(destination.vid(), vid(2));
                })),
            },
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        let adapter_ref = adapter.borrow();
        assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
        assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 1);
    }

    #[test]
    fn coercion_on_folded_edge() {
        let input_name = "coercion_on_folded_edge";

        let adapter = TestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert!(info.coerced_to_type().is_none());
                    assert_eq!(info.vid(), vid(1));

                    let edge = info.first_edge("predecessor").expect("no edge returned");
                    let destination = edge.destination();

                    assert_eq!(
                        Some("Prime"),
                        destination.coerced_to_type().map(|x| x.as_ref()),
                    );
                })),
            },
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(info.origin_vid(), vid(1));

                    let destination = info.destination();
                    assert_eq!(destination.vid(), vid(2));
                    assert_eq!(
                        Some("Prime"),
                        destination.coerced_to_type().map(|x| x.as_ref()),
                    );
                })),
            },
            on_type_coercion: btreemap! {
                vid(2) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(Some("Prime"), info.coerced_to_type().map(|x| x.as_ref()));
                    assert_eq!(info.vid(), vid(2));
                })),
            },
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        let adapter_ref = adapter.borrow();
        assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
        assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 1);

        // it's inside a @fold with 7 elements
        assert_eq!(adapter_ref.on_type_coercion[&vid(2)].calls, 7);
    }

    mod static_property_values {
        use std::ops::Bound;

        use crate::{
            interpreter::hints::{CandidateValue, Range},
            ir::FieldValue,
        };

        use super::{
            super::VertexInfo, eid, run_query, vid, ResolveEdgeInfoFn, ResolveInfoFn, TestAdapter,
            TrackCalls,
        };

        #[test]
        fn simple_filter() {
            let input_name = "simple_filter";

            let adapter = TestAdapter {
                on_starting_vertices: btreemap! {
                    vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                        assert!(info.coerced_to_type().is_none());
                        assert_eq!(vid(1), info.vid());

                        assert_eq!(None, info.statically_known_property("name"));
                        assert_eq!(
                            Some(CandidateValue::Single(&FieldValue::Int64(3))),
                            info.statically_known_property("value"),
                        );
                    })),
                },
                ..Default::default()
            };

            let adapter = run_query(adapter, input_name);
            let adapter_ref = adapter.borrow();
            assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
        }

        #[test]
        fn typename_filter() {
            let input_name = "typename_filter";

            let adapter = TestAdapter {
                on_starting_vertices: btreemap! {
                    vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                        assert!(info.coerced_to_type().is_none());
                        assert_eq!(vid(1), info.vid());

                        assert_eq!(None, info.statically_known_property("value"));
                        assert_eq!(
                            Some(CandidateValue::Single(&FieldValue::String("Prime".into()))),
                            info.statically_known_property("__typename"),
                        );
                    })),
                },
                ..Default::default()
            };

            let adapter = run_query(adapter, input_name);
            let adapter_ref = adapter.borrow();
            assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
        }

        #[test]
        fn filter_op_one_of() {
            let input_name = "filter_op_one_of";

            let adapter = TestAdapter {
                on_starting_vertices: btreemap! {
                    vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                        assert!(info.coerced_to_type().is_none());
                        assert_eq!(vid(1), info.vid());

                        assert_eq!(
                            Some(CandidateValue::Multiple(vec![
                                &"fourteen".into(),
                                &"fifteen".into(),
                            ])),
                            info.statically_known_property("name"),
                        );
                    })),
                },
                ..Default::default()
            };

            let adapter = run_query(adapter, input_name);
            let adapter_ref = adapter.borrow();
            assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
        }

        #[test]
        fn filter_op_less_than() {
            let input_name = "filter_op_less_than";

            let adapter = TestAdapter {
                on_starting_vertices: btreemap! {
                    vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                        assert!(info.coerced_to_type().is_none());
                        assert_eq!(vid(1), info.vid());

                        assert_eq!(
                            Some(CandidateValue::Range(Range::with_end(Bound::Excluded(&FieldValue::Int64(9)), true))),
                            info.statically_known_property("value"),
                        );
                    })),
                },
                ..Default::default()
            };

            let adapter = run_query(adapter, input_name);
            let adapter_ref = adapter.borrow();
            assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
        }

        #[test]
        fn filter_op_less_or_equal() {
            let input_name = "filter_op_less_or_equal";

            let adapter = TestAdapter {
                on_starting_vertices: btreemap! {
                    vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                        assert!(info.coerced_to_type().is_none());
                        assert_eq!(vid(1), info.vid());

                        assert_eq!(
                            Some(CandidateValue::Range(Range::with_end(Bound::Included(&FieldValue::Int64(8)), true))),
                            info.statically_known_property("value"),
                        );
                    })),
                },
                ..Default::default()
            };

            let adapter = run_query(adapter, input_name);
            let adapter_ref = adapter.borrow();
            assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
        }

        #[test]
        fn filter_op_greater_than() {
            let input_name = "filter_op_greater_than";

            let adapter = TestAdapter {
                on_starting_vertices: btreemap! {
                    vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                        assert!(info.coerced_to_type().is_none());
                        assert_eq!(vid(1), info.vid());

                        let edge_info = info.first_edge("multiple").expect("no 'multiple' edge info");
                        let neighbor = edge_info.destination();

                        assert_eq!(vid(2), neighbor.vid());
                        assert_eq!(
                            Some(CandidateValue::Range(Range::with_start(Bound::Excluded(&FieldValue::Int64(25)), true))),
                            neighbor.statically_known_property("value"),
                        );
                    })),
                },
                on_edge_resolver: btreemap! {
                    eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                        assert_eq!(eid(1), info.eid());
                        assert_eq!(vid(1), info.origin_vid());
                        assert_eq!(vid(2), info.destination_vid());
                        assert_eq!(Some(&FieldValue::Int64(4)), info.edge().parameters().get("max"));

                        let neighbor = info.destination();
                        assert_eq!(vid(2), neighbor.vid());
                        assert_eq!(
                            Some(CandidateValue::Range(Range::with_start(Bound::Excluded(&FieldValue::Int64(25)), true))),
                            neighbor.statically_known_property("value"),
                        );
                    }))
                },
                ..Default::default()
            };

            let adapter = run_query(adapter, input_name);
            let adapter_ref = adapter.borrow();
            assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
            assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 1);
        }

        #[test]
        fn filter_op_greater_or_equal() {
            let input_name = "filter_op_greater_or_equal";

            let adapter = TestAdapter {
                on_starting_vertices: btreemap! {
                    vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                        assert!(info.coerced_to_type().is_none());
                        assert_eq!(vid(1), info.vid());

                        let edge_info = info.first_edge("multiple").expect("no 'multiple' edge info");
                        let neighbor = edge_info.destination();

                        assert_eq!(vid(2), neighbor.vid());
                        assert_eq!(
                            Some(CandidateValue::Range(Range::with_start(Bound::Included(&FieldValue::Int64(24)), true))),
                            neighbor.statically_known_property("value"),
                        );
                    })),
                },
                on_edge_resolver: btreemap! {
                    eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                        assert_eq!(eid(1), info.eid());
                        assert_eq!(vid(1), info.origin_vid());
                        assert_eq!(vid(2), info.destination_vid());
                        assert_eq!(Some(&FieldValue::Int64(4)), info.edge().parameters().get("max"));

                        let neighbor = info.destination();
                        assert_eq!(vid(2), neighbor.vid());
                        assert_eq!(
                            Some(CandidateValue::Range(Range::with_start(Bound::Included(&FieldValue::Int64(24)), true))),
                            neighbor.statically_known_property("value"),
                        );
                    }))
                },
                ..Default::default()
            };

            let adapter = run_query(adapter, input_name);
            let adapter_ref = adapter.borrow();
            assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
            assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 1);
        }

        #[test]
        fn recurse_then_filter_depth_one() {
            let input_name = "recurse_then_filter_depth_one";

            let adapter = TestAdapter {
                on_starting_vertices: btreemap! {
                    vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                        assert_eq!(vid(1), info.vid());
                        assert!(info.coerced_to_type().is_none());

                        let edge_info = info.first_edge("successor").expect("no 'successor' edge info");
                        let neighbor = edge_info.destination();

                        assert_eq!(vid(2), neighbor.vid());

                        // This value here is statically known, since the recursion is `depth: 1`.
                        // The edge is recursively traversed only once, i.e. the selected vertices
                        // are the starting vertex and the neighbors, so the filter is binding
                        // to all neighboring vertices.
                        assert_eq!(
                            Some(CandidateValue::Single(&FieldValue::Int64(6))),
                            neighbor.statically_known_property("value"),
                        );
                    })),
                },
                on_edge_resolver: btreemap! {
                    eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                        let destination = info.destination();
                        assert_eq!(vid(2), destination.vid());
                        assert!(destination.coerced_to_type().is_none());

                        // This value here is statically known, since the recursion is `depth: 1`.
                        // The edge is recursively traversed only once, i.e. the selected vertices
                        // are the starting vertex and the neighbors, so the filter is binding
                        // to all neighboring vertices.
                        assert_eq!(
                            Some(CandidateValue::Single(&FieldValue::Int64(6))),
                            destination.statically_known_property("value"),
                        );
                    })),
                },
                ..Default::default()
            };

            let adapter = run_query(adapter, input_name);
            let adapter_ref = adapter.borrow();
            assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
            assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 1);
        }

        #[test]
        fn recurse_then_filter_depth_two() {
            let input_name = "recurse_then_filter_depth_two";

            let adapter = TestAdapter {
                on_starting_vertices: btreemap! {
                    vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                        assert_eq!(vid(1), info.vid());
                        assert!(info.coerced_to_type().is_none());

                        let edge_info = info.first_edge("successor").expect("no 'successor' edge info");
                        let neighbor = edge_info.destination();

                        assert_eq!(vid(2), neighbor.vid());

                        // This value actually *isn't* statically known, because it isn't limited.
                        // The edge is recursively traversed up to 2 times, and the filter is
                        // applied only afterward.
                        //
                        // The "middle" vertices in the recursion are not required to satisfy
                        // the filter, and including the filter's value here would be a footgun.
                        assert_eq!(None, neighbor.statically_known_property("value"));
                    })),
                },
                on_edge_resolver: btreemap! {
                    eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                        let destination = info.destination();
                        assert_eq!(vid(2), destination.vid());
                        assert!(destination.coerced_to_type().is_none());

                        // This value actually *isn't* statically known, because it isn't limited.
                        // The edge is recursively traversed up to 2 times, and the filter is
                        // applied only afterward.
                        //
                        // The "middle" vertices in the recursion are not required to satisfy
                        // the filter, and including the filter's value here would be a footgun.
                        assert_eq!(None, destination.statically_known_property("value"));
                    })),
                },
                ..Default::default()
            };

            let adapter = run_query(adapter, input_name);
            let adapter_ref = adapter.borrow();
            assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
            assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 2);
        }
    }

    mod dynamic_property_values {
        use std::{collections::BTreeMap, ops::Bound, sync::Arc};

        use itertools::{EitherOrBoth, Itertools};

        use crate::{
            interpreter::{
                hints::{CandidateValue, Range},
                Adapter, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, ResolveInfo,
                VertexInfo, VertexIterator,
            },
            ir::{Eid, FieldValue, Vid},
            numbers_interpreter::NumbersAdapter,
        };

        use super::*;

        type CtxIter = ContextIterator<'static, <DynamicTestAdapter as Adapter<'static>>::Vertex>;
        type ResolveInfoFn = Box<dyn FnMut(&mut NumbersAdapter, CtxIter, &ResolveInfo) -> CtxIter>;
        type ResolveEdgeInfoFn =
            Box<dyn FnMut(&mut NumbersAdapter, CtxIter, &ResolveEdgeInfo) -> CtxIter>;

        #[derive(Default)]
        struct TrackCalls<F> {
            underlying: F,
            calls: usize,
        }

        impl<F> TrackCalls<F> {
            fn new_underlying(underlying: F) -> Self {
                Self {
                    underlying,
                    calls: 0,
                }
            }
        }

        impl TrackCalls<ResolveInfoFn> {
            fn call(
                &mut self,
                adapter: &mut NumbersAdapter,
                ctxs: CtxIter,
                info: &ResolveInfo,
            ) -> CtxIter {
                self.calls += 1;
                (self.underlying)(adapter, ctxs, info)
            }
        }

        impl TrackCalls<ResolveEdgeInfoFn> {
            fn call(
                &mut self,
                adapter: &mut NumbersAdapter,
                ctxs: CtxIter,
                info: &ResolveEdgeInfo,
            ) -> CtxIter {
                self.calls += 1;
                (self.underlying)(adapter, ctxs, info)
            }
        }

        struct DynamicTestAdapter {
            on_starting_vertices: BTreeMap<Vid, TrackCalls<ResolveInfoFn>>,
            on_property_resolver: BTreeMap<Vid, TrackCalls<ResolveInfoFn>>,
            on_edge_resolver: BTreeMap<Eid, TrackCalls<ResolveEdgeInfoFn>>,
            on_type_coercion: BTreeMap<Vid, TrackCalls<ResolveInfoFn>>,
            inner: NumbersAdapter,
        }

        impl DynamicTestAdapter {
            fn new() -> Self {
                Self {
                    inner: NumbersAdapter::new(),
                    on_starting_vertices: Default::default(),
                    on_property_resolver: Default::default(),
                    on_edge_resolver: Default::default(),
                    on_type_coercion: Default::default(),
                }
            }
        }

        impl Default for DynamicTestAdapter {
            fn default() -> Self {
                Self::new()
            }
        }

        impl Adapter<'static> for DynamicTestAdapter {
            type Vertex = <NumbersAdapter as Adapter<'static>>::Vertex;

            fn resolve_starting_vertices(
                &mut self,
                edge_name: &Arc<str>,
                parameters: &crate::ir::EdgeParameters,
                resolve_info: &super::ResolveInfo,
            ) -> VertexIterator<'static, Self::Vertex> {
                if let Some(x) = self.on_starting_vertices.get_mut(&resolve_info.current_vid) {
                    // the starting vertices call doesn't have an iterator
                    let _ = x.call(&mut self.inner, Box::new(std::iter::empty()), resolve_info);
                }
                self.inner
                    .resolve_starting_vertices(edge_name, parameters, resolve_info)
            }

            fn resolve_property(
                &mut self,
                mut contexts: ContextIterator<'static, Self::Vertex>,
                type_name: &Arc<str>,
                property_name: &Arc<str>,
                resolve_info: &super::ResolveInfo,
            ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
                if let Some(x) = self.on_property_resolver.get_mut(&resolve_info.current_vid) {
                    contexts = x.call(&mut self.inner, contexts, resolve_info);
                }
                self.inner
                    .resolve_property(contexts, type_name, property_name, resolve_info)
            }

            fn resolve_neighbors(
                &mut self,
                mut contexts: ContextIterator<'static, Self::Vertex>,
                type_name: &Arc<str>,
                edge_name: &Arc<str>,
                parameters: &crate::ir::EdgeParameters,
                resolve_info: &super::ResolveEdgeInfo,
            ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>>
            {
                if let Some(x) = self.on_edge_resolver.get_mut(&resolve_info.eid()) {
                    contexts = x.call(&mut self.inner, contexts, resolve_info);
                }
                self.inner.resolve_neighbors(
                    contexts,
                    type_name,
                    edge_name,
                    parameters,
                    resolve_info,
                )
            }

            fn resolve_coercion(
                &mut self,
                mut contexts: ContextIterator<'static, Self::Vertex>,
                type_name: &Arc<str>,
                coerce_to_type: &Arc<str>,
                resolve_info: &super::ResolveInfo,
            ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
                if let Some(x) = self.on_type_coercion.get_mut(&resolve_info.current_vid) {
                    contexts = x.call(&mut self.inner, contexts, resolve_info);
                }
                self.inner
                    .resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
            }
        }

        /// Ensure that statically-known values are propagated and automatically
        /// intersected with dynamically-known property values.
        #[test]
        fn static_and_dynamic_filter() {
            let input_name = "static_and_dynamic_filter";

            let adapter = DynamicTestAdapter {
                on_starting_vertices: btreemap! {
                    vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                        assert!(info.coerced_to_type().is_none());
                        assert_eq!(vid(1), info.vid());

                        // This property isn't known or needed at all.
                        assert_eq!(None, info.dynamically_known_property("name"));

                        let edge = info.first_edge("successor").expect("no 'successor' edge");
                        let destination = edge.destination();
                        // We haven't resolved Vid 1 yet, so this property
                        // isn't dynamically known yet.
                        assert_eq!(None, destination.dynamically_known_property("value"));

                        ctxs
                    })),
                },
                on_edge_resolver: btreemap! {
                    eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|adapter, ctxs, info| {
                        assert_eq!(eid(1), info.eid());
                        assert_eq!(vid(1), info.origin_vid());
                        assert_eq!(vid(2), info.destination_vid());

                        let destination = info.destination();

                        let expected_values = vec![
                            CandidateValue::Single(FieldValue::Int64(3)),
                            CandidateValue::Multiple(vec![FieldValue::Int64(3), FieldValue::Int64(4)]),
                        ];
                        let value_candidate = destination.dynamically_known_property("value");
                        Box::new(value_candidate
                            .expect("no dynamic candidate for 'value' property")
                            .resolve(adapter, ctxs)
                            .zip_longest(expected_values.into_iter())
                            .map(move |data| {
                                if let EitherOrBoth::Both((ctx, value), expected_value) = data {
                                    assert_eq!(expected_value, value);
                                    ctx
                                } else {
                                    panic!("unexpected iterator outcome: {data:?}")
                                }
                            }))
                    })),
                },
                ..Default::default()
            };

            let adapter = run_query(adapter, input_name);
            let adapter_ref = adapter.borrow();
            assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
            assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 1);
        }

        /// The filters are binding since the recursion is depth 1.
        /// This is the analogous dynamic case of the static-only test case
        /// in super::static_property_values::recurse_then_filter_depth_one().
        #[test]
        fn recurse_then_filter_on_tag_depth_one() {
            let input_name = "recurse_then_filter_on_tag_depth_one";

            let adapter = DynamicTestAdapter {
                on_starting_vertices: btreemap! {
                    vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                        assert!(info.coerced_to_type().is_none());
                        assert_eq!(vid(1), info.vid());

                        let edge = info.first_edge("successor").expect("no 'successor' edge");
                        let destination = edge.destination();
                        // We haven't resolved Vid 1 yet, so this property
                        // isn't dynamically known yet.
                        assert_eq!(None, destination.dynamically_known_property("value"));

                        ctxs
                    })),
                },
                on_edge_resolver: btreemap! {
                    eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|adapter, ctxs, info| {
                        assert_eq!(eid(1), info.eid());
                        assert_eq!(vid(1), info.origin_vid());
                        assert_eq!(vid(2), info.destination_vid());

                        let destination = info.destination();

                        let expected_values = vec![
                            CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(0)), true)),
                            CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(1)), true)),
                            CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(2)), true)),
                            CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(3)), true)),
                            CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(4)), true)),
                            CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(5)), true)),
                        ];
                        let value_candidate = destination.dynamically_known_property("value");
                        Box::new(value_candidate
                            .expect("no dynamic candidate for 'value' property")
                            .resolve(adapter, ctxs)
                            .zip_longest(expected_values.into_iter())
                            .map(move |data| {
                                if let EitherOrBoth::Both((ctx, value), expected_value) = data {
                                    assert_eq!(expected_value, value);
                                    ctx
                                } else {
                                    panic!("unexpected iterator outcome: {data:?}")
                                }
                            }))
                    })),
                },
                ..Default::default()
            };

            let adapter = run_query(adapter, input_name);
            let adapter_ref = adapter.borrow();
            assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
            assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 1);
        }

        /// The filters are not binding since the recursion is depth 2+.
        /// This is the analogous dynamic case of the static-only test case
        /// in super::static_property_values::recurse_then_filter_depth_two().
        #[test]
        fn recurse_then_filter_on_tag_depth_two() {
            let input_name = "recurse_then_filter_on_tag_depth_two";

            let adapter = DynamicTestAdapter {
                on_starting_vertices: btreemap! {
                    vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                        assert!(info.coerced_to_type().is_none());
                        assert_eq!(vid(1), info.vid());

                        let edge = info.first_edge("successor").expect("no 'successor' edge");
                        let destination = edge.destination();
                        // We haven't resolved Vid 1 yet, so this property
                        // isn't dynamically known yet.
                        assert_eq!(None, destination.dynamically_known_property("value"));

                        ctxs
                    })),
                },
                on_edge_resolver: btreemap! {
                    eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                        assert_eq!(eid(1), info.eid());
                        assert_eq!(vid(1), info.origin_vid());
                        assert_eq!(vid(2), info.destination_vid());

                        let destination = info.destination();

                        let value_candidate = destination.dynamically_known_property("value");
                        assert_eq!(None, value_candidate);
                        ctxs
                    })),
                },
                ..Default::default()
            };

            let adapter = run_query(adapter, input_name);
            let adapter_ref = adapter.borrow();
            assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
            assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 2);
        }
    }
}
