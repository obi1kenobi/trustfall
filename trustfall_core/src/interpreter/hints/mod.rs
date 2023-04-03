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
    pub(crate) fn new(query: InterpretedQuery, current_vid: Vid) -> Self {
        Self { query, current_vid }
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
    fn execution_frontier(&self) -> Bound<Vid> {
        Bound::Included(self.current_vid)
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
            execution_frontier: Bound::Included(self.current_vid),
            starting_vertex: self.current_vid,
            neighbor_vertex: edge.to_vid,
            neighbor_path: vec![edge.eid],
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
            execution_frontier: Bound::Included(self.current_vid),
            starting_vertex: self.current_vid,
            neighbor_vertex: fold.to_vid,
            neighbor_path: vec![fold.eid],
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
}

impl InternalVertexInfo for NeighborInfo {
    #[inline]
    fn query(&self) -> &InterpretedQuery {
        &self.query
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

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell, collections::BTreeMap, num::NonZeroUsize, path::PathBuf, rc::Rc, sync::Arc,
    };

    use super::{ResolveEdgeInfo, ResolveInfo};
    use crate::{
        interpreter::{
            execution::interpret_ir, Adapter, ContextIterator, ContextOutcomeIterator, DataContext,
            VertexInfo, VertexIterator,
        },
        ir::{Eid, FieldValue, Recursive, Vid},
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

    #[derive(Default)]
    struct TestAdapter {
        on_starting_vertices: BTreeMap<Vid, TrackCalls<ResolveInfoFn>>,
        on_property_resolver: BTreeMap<Vid, TrackCalls<ResolveInfoFn>>,
        on_edge_resolver: BTreeMap<Eid, TrackCalls<ResolveEdgeInfoFn>>,
        on_type_coercion: BTreeMap<Vid, TrackCalls<ResolveInfoFn>>,
    }

    impl Adapter<'static> for TestAdapter {
        type Vertex = ();

        fn resolve_starting_vertices(
            &mut self,
            _edge_name: &Arc<str>,
            _parameters: &crate::ir::EdgeParameters,
            resolve_info: &super::ResolveInfo,
        ) -> VertexIterator<'static, Self::Vertex> {
            if let Some(x) = self.on_starting_vertices.get_mut(&resolve_info.current_vid) {
                x.call(resolve_info)
            }
            Box::new(std::iter::empty())
        }

        fn resolve_property(
            &mut self,
            contexts: ContextIterator<'static, Self::Vertex>,
            _type_name: &Arc<str>,
            _property_name: &Arc<str>,
            resolve_info: &super::ResolveInfo,
        ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
            if let Some(x) = self.on_property_resolver.get_mut(&resolve_info.current_vid) {
                x.call(resolve_info)
            }
            Box::new(contexts.map(|ctx| (ctx, FieldValue::Null)))
        }

        fn resolve_neighbors(
            &mut self,
            contexts: ContextIterator<'static, Self::Vertex>,
            _type_name: &Arc<str>,
            _edge_name: &Arc<str>,
            _parameters: &crate::ir::EdgeParameters,
            resolve_info: &super::ResolveEdgeInfo,
        ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>>
        {
            if let Some(x) = self.on_edge_resolver.get_mut(&resolve_info.eid()) {
                x.call(resolve_info)
            }
            Box::new(
                contexts.map(|ctx| -> (DataContext<()>, Box<dyn Iterator<Item = ()>>) {
                    (ctx, Box::new(std::iter::empty()))
                }),
            )
        }

        fn resolve_coercion(
            &mut self,
            contexts: ContextIterator<'static, Self::Vertex>,
            _type_name: &Arc<str>,
            _coerce_to_type: &Arc<str>,
            resolve_info: &super::ResolveInfo,
        ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
            if let Some(x) = self.on_type_coercion.get_mut(&resolve_info.current_vid) {
                x.call(resolve_info)
            }
            Box::new(contexts.map(|ctx| (ctx, false)))
        }
    }

    fn get_ir_for_named_query(stem: &str) -> TestIRQuery {
        let mut input_path = PathBuf::from("test_data/tests/valid_queries");
        input_path.push(format!("{stem}.ir.ron"));
        dbg!(&input_path);
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

    fn run_query(adapter: TestAdapter, input_name: &str) -> Rc<RefCell<TestAdapter>> {
        let input = get_ir_for_named_query(input_name);
        let adapter = Rc::from(RefCell::new(adapter));
        interpret_ir(
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
        .next();
        adapter
    }

    #[test]
    fn coercion_at_root() {
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

    mod property_values {
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

        // #[test]
        // fn recurse_then_filter_intermediate() {
        //     let input_name = "recurse_then_filter_intermediate";

        //     let adapter = TestAdapter {
        //         on_starting_vertices: btreemap! {
        //             vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
        //                 assert_eq!(vid(1), info.vid());
        //                 assert!(info.coerced_to_type().is_none());

        //                 let edge_info = info.first_edge("successor").expect("no 'successor' edge info");
        //                 let neighbor = edge_info.destination();

        //                 assert_eq!(vid(2), neighbor.vid());

        //                 // This value actually *isn't* statically known, because it isn't limited.
        //                 // The edge is recursively traversed up to 3 times, and the filter is
        //                 // applied only afterward.
        //                 //
        //                 // The "middle" vertices in the recursion are not required to satisfy
        //                 // the filter, and including the filter's value here would be a footgun.
        //                 assert_eq!(None, neighbor.statically_known_property("value"));
        //             })),
        //         },
        //         on_edge_resolver: btreemap! {
        //             eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
        //                 let destination = info.destination();
        //                 assert_eq!(vid(2), destination.vid());
        //                 assert!(destination.coerced_to_type().is_none());

        //                 // This value actually *isn't* statically known, because it isn't limited.
        //                 // The edge is recursively traversed up to 3 times, and the filter is
        //                 // applied only afterward.
        //                 //
        //                 // The "middle" vertices in the recursion are not required to satisfy
        //                 // the filter, and including the filter's value here would be a footgun.
        //                 assert_eq!(None, destination.statically_known_property("value"));
        //             })),
        //         },
        //         ..Default::default()
        //     };

        //     let adapter = run_query(adapter, input_name);
        //     let adapter_ref = adapter.borrow();
        //     assert_eq!(adapter_ref.on_starting_vertices[&vid(1)].calls, 1);
        //     assert_eq!(adapter_ref.on_edge_resolver[&eid(1)].calls, 3);
        // }
    }
}
