use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

use super::InterpretedQuery;
use crate::ir::{
    EdgeKind, EdgeParameters, Eid, FieldValue, IREdge, IRFold, IRQuery, IRQueryComponent, IRVertex,
    Output, Recursive, Vid,
};

/// Information about what some query is looking for at a specific vertex in the query structure.
#[cfg_attr(docsrs, doc(notable_trait))]
pub trait VertexInfo {
    /// The unique ID of the vertex this `VertexInfo` describes.
    fn vid(&self) -> Vid;

    /// The type coercion the query applied at this vertex, if any.
    fn coerced_to_type(&self) -> Option<&Arc<str>>;

    /// Returns an iterator of all the edges by that name originating from this vertex.
    fn edges_with_name<'a>(&'a self, name: &'a str) -> Box<dyn Iterator<Item = EdgeInfo> + 'a>;

    /// Returns info for the first edge by the given name that is *mandatory*:
    /// this vertex must contain the edge, or it will be discarded during query processing.
    ///
    /// Edges marked `@optional`, `@fold`, or `@recurse` are not mandatory:
    /// - `@optional` edges that don't exist produce `null` outputs.
    /// - `@fold` edges that don't exist produce empty lists.
    /// - `@recurse` always starts at depth 0 (i.e. returning the *current* vertex),
    ///   so the edge does not have to exist.
    fn first_mandatory_edge(&self, name: &str) -> Option<EdgeInfo>;

    /// Returns info for the first edge by the given name.
    fn first_edge(&self, name: &str) -> Option<EdgeInfo>;
}

trait InternalVertexInfo {
    fn current_vertex(&self) -> &IRVertex;

    fn current_component(&self) -> &IRQueryComponent;

    fn make_non_folded_edge_info(&self, edge: &IREdge) -> EdgeInfo;

    fn make_folded_edge_info(&self, fold: &IRFold) -> EdgeInfo;
}

impl<T: InternalVertexInfo> VertexInfo for T {
    fn vid(&self) -> Vid {
        self.current_vertex().vid
    }

    fn coerced_to_type(&self) -> Option<&Arc<str>> {
        let vertex = self.current_vertex();
        if vertex.coerced_from_type.is_some() {
            Some(&vertex.type_name)
        } else {
            None
        }
    }

    fn edges_with_name<'a>(&'a self, name: &'a str) -> Box<dyn Iterator<Item = EdgeInfo> + 'a> {
        let component = self.current_component();
        let current_vid = self.current_vertex().vid;

        let non_folded_edges = component
            .edges
            .values()
            .filter(move |edge| edge.from_vid == current_vid && edge.edge_name.as_ref() == name)
            .map(|edge| self.make_non_folded_edge_info(edge.as_ref()));
        let folded_edges = component
            .folds
            .values()
            .filter(move |fold| fold.from_vid == current_vid && fold.edge_name.as_ref() == name)
            .map(|fold| self.make_folded_edge_info(fold.as_ref()));

        Box::new(non_folded_edges.chain(folded_edges))
    }

    fn first_mandatory_edge(&self, name: &str) -> Option<EdgeInfo> {
        self.edges_with_name(name)
            .find(|edge| !edge.folded && !edge.optional && edge.recursive.is_none())
    }

    fn first_edge(&self, name: &str) -> Option<EdgeInfo> {
        self.edges_with_name(name).next()
    }
}

/// Information about the query being processed.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryInfo {
    query: InterpretedQuery,
    pub(crate) current_vertex: Vid,
    pub(crate) crossing_eid: Option<Eid>,
}

impl QueryInfo {
    pub(crate) fn new(
        query: InterpretedQuery,
        current_vertex: Vid,
        crossing_eid: Option<Eid>,
    ) -> Self {
        Self {
            query,
            current_vertex,
            crossing_eid,
        }
    }

    pub(crate) fn ir_query(&self) -> &IRQuery {
        &self.query.indexed_query.ir_query
    }

    /// All the names and type information of the output data of this query.
    #[inline]
    pub fn outputs(&self) -> &BTreeMap<Arc<str>, Output> {
        &self.query.indexed_query.outputs
    }

    /// The arguments with which the query was executed.
    #[inline]
    pub fn query_arguments(&self) -> &Arc<BTreeMap<Arc<str>, FieldValue>> {
        &self.query.arguments
    }

    /// The unique ID of the vertex at the query location where this [`QueryInfo`] was provided.
    #[inline]
    pub fn origin_vid(&self) -> Vid {
        self.current_vertex
    }

    /// Info about the specific place in the query where this [`QueryInfo`] was provided.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn here(&self) -> LocalInfo {
        LocalInfo {
            query_info: self.clone(),
            current_vertex: self.current_vertex,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryInfoAlongEdge {
    query_info: QueryInfo,
}

impl QueryInfoAlongEdge {
    pub(crate) fn new(query_info: QueryInfo) -> Self {
        Self { query_info }
    }

    /// All the names and type information of the output data of this query.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn outputs(&self) -> &BTreeMap<Arc<str>, Output> {
        &self.query_info.query.indexed_query.outputs
    }

    /// The arguments with which the query was executed.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn query_arguments(&self) -> &Arc<BTreeMap<Arc<str>, FieldValue>> {
        &self.query_info.query.arguments
    }

    /// The unique ID of this edge within its query.
    #[inline]
    pub fn eid(&self) -> Eid {
        self.query_info
            .crossing_eid
            .expect("QueryInfoAlongEdge constructed from QueryInfo that is not at an edge")
    }

    /// The unique ID of the vertex where the edge in this operation begins.
    #[inline]
    pub fn origin_vid(&self) -> Vid {
        self.query_info.current_vertex
    }

    /// The unique ID of the vertex to which this edge points.
    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[inline]
    pub fn destination_vid(&self) -> Vid {
        self.destination().neighbor_vertex
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
        match &self.query_info.query.indexed_query.eids[&eid] {
            EdgeKind::Regular(regular) => EdgeInfo {
                eid,
                parameters: regular.parameters.clone(),
                optional: regular.optional,
                recursive: regular.recursive.clone(),
                folded: false,
                destination: NeighborInfo {
                    query_info: self.query_info.clone(),
                    starting_vertex: self.query_info.current_vertex,
                    neighbor_vertex: regular.to_vid,
                    neighbor_path: vec![eid],
                },
            },
            EdgeKind::Fold(fold) => EdgeInfo {
                eid,
                parameters: fold.parameters.clone(),
                optional: false,
                recursive: None,
                folded: true,
                destination: NeighborInfo {
                    query_info: self.query_info.clone(),
                    starting_vertex: self.query_info.current_vertex,
                    neighbor_vertex: fold.to_vid,
                    neighbor_path: vec![eid],
                },
            },
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalInfo {
    query_info: QueryInfo,
    current_vertex: Vid,
}

impl InternalVertexInfo for LocalInfo {
    #[inline]
    fn current_vertex(&self) -> &IRVertex {
        &self.current_component().vertices[&self.current_vertex]
    }

    #[inline]
    fn current_component(&self) -> &IRQueryComponent {
        &self.query_info.query.indexed_query.vids[&self.current_vertex]
    }

    fn make_non_folded_edge_info(&self, edge: &IREdge) -> EdgeInfo {
        let neighboring_info = NeighborInfo {
            query_info: self.query_info.clone(),
            starting_vertex: self.current_vertex,
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
            query_info: self.query_info.clone(),
            starting_vertex: self.current_vertex,
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
    query_info: QueryInfo,
    starting_vertex: Vid,
    neighbor_vertex: Vid,
    neighbor_path: Vec<Eid>,
}

impl InternalVertexInfo for NeighborInfo {
    #[inline]
    fn current_vertex(&self) -> &IRVertex {
        &self.current_component().vertices[&self.neighbor_vertex]
    }

    #[inline]
    fn current_component(&self) -> &IRQueryComponent {
        &self.query_info.query.indexed_query.vids[&self.neighbor_vertex]
    }

    fn make_non_folded_edge_info(&self, edge: &IREdge) -> EdgeInfo {
        let mut neighbor_path = self.neighbor_path.clone();
        neighbor_path.push(edge.eid);
        let neighboring_info = NeighborInfo {
            query_info: self.query_info.clone(),
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
            query_info: self.query_info.clone(),
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

    use super::{QueryInfo, QueryInfoAlongEdge};
    use crate::{
        interpreter::{
            execution::interpret_ir, Adapter, ContextIterator, ContextOutcomeIterator, DataContext,
            VertexInfo, VertexIterator,
        },
        ir::{Eid, FieldValue, Recursive, Vid},
        util::{TestIRQuery, TestIRQueryResult},
    };

    type QueryInfoFn = Box<dyn FnMut(&QueryInfo)>;
    type QueryInfoAlongEdgeFn = Box<dyn FnMut(&QueryInfoAlongEdge)>;

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

    impl TrackCalls<QueryInfoFn> {
        fn call(&mut self, info: &QueryInfo) {
            self.calls += 1;
            if let Some(underlying) = self.underlying.as_mut() {
                underlying(info);
            }
        }
    }

    impl TrackCalls<QueryInfoAlongEdgeFn> {
        fn call(&mut self, info: &QueryInfoAlongEdge) {
            self.calls += 1;
            if let Some(underlying) = self.underlying.as_mut() {
                underlying(info);
            }
        }
    }

    #[derive(Default)]
    struct TestAdapter {
        on_starting_vertices: BTreeMap<Vid, TrackCalls<QueryInfoFn>>,
        on_property_resolver: BTreeMap<Vid, TrackCalls<QueryInfoFn>>,
        on_edge_resolver: BTreeMap<Eid, TrackCalls<QueryInfoAlongEdgeFn>>,
        on_type_coercion: BTreeMap<Vid, TrackCalls<QueryInfoFn>>,
    }

    impl Adapter<'static> for TestAdapter {
        type Vertex = ();

        fn resolve_starting_vertices(
            &mut self,
            _edge_name: &Arc<str>,
            _parameters: &crate::ir::EdgeParameters,
            query_info: &super::QueryInfo,
        ) -> VertexIterator<'static, Self::Vertex> {
            if let Some(x) = self
                .on_starting_vertices
                .get_mut(&query_info.current_vertex)
            {
                x.call(query_info)
            }
            Box::new(std::iter::empty())
        }

        fn resolve_property(
            &mut self,
            contexts: ContextIterator<'static, Self::Vertex>,
            _type_name: &Arc<str>,
            _property_name: &Arc<str>,
            query_info: &super::QueryInfo,
        ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
            if let Some(x) = self
                .on_property_resolver
                .get_mut(&query_info.current_vertex)
            {
                x.call(query_info)
            }
            Box::new(contexts.map(|ctx| (ctx, FieldValue::Null)))
        }

        fn resolve_neighbors(
            &mut self,
            contexts: ContextIterator<'static, Self::Vertex>,
            _type_name: &Arc<str>,
            _edge_name: &Arc<str>,
            _parameters: &crate::ir::EdgeParameters,
            query_info: &super::QueryInfoAlongEdge,
        ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>>
        {
            if let Some(x) = self.on_edge_resolver.get_mut(&query_info.eid()) {
                x.call(query_info)
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
            query_info: &super::QueryInfo,
        ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
            if let Some(x) = self.on_type_coercion.get_mut(&query_info.current_vertex) {
                x.call(query_info)
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
                vid(1) => TrackCalls::<QueryInfoFn>::new_underlying(Box::new(|info| {
                    let local_info = info.here();
                    assert_eq!(local_info.coerced_to_type().map(|x| x.as_ref()), Some("Prime"));
                    assert_eq!(local_info.vid(), vid(1));
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
                vid(1) => TrackCalls::<QueryInfoFn>::new_underlying(Box::new(|info| {
                    let local_info = info.here();
                    assert_eq!(local_info.coerced_to_type().map(|x| x.as_ref()), Some("Prime"));
                    assert_eq!(local_info.vid(), vid(1));
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
                vid(1) => TrackCalls::<QueryInfoFn>::new_underlying(Box::new(|info| {
                    let local_info = info.here();
                    assert!(local_info.coerced_to_type().is_none());
                    assert_eq!(local_info.vid(), vid(1));

                    let edges: Vec<_> = local_info.edges_with_name("successor").collect();
                    assert_eq!(edges.len(), 2);

                    let first_edge = &edges[0];
                    let second_edge = &edges[1];
                    assert_eq!(first_edge.eid(), eid(1));
                    assert!(first_edge.parameters().is_empty());
                    assert_eq!(second_edge.eid(), eid(2));
                    assert!(second_edge.parameters().is_empty());

                    // The "first_*" methods produce the same outcome as the iterator-based method.
                    assert_eq!(first_edge, &local_info.first_edge("successor").unwrap());
                    assert_eq!(first_edge, &local_info.first_mandatory_edge("successor").unwrap());

                    let first_destination = first_edge.destination();
                    let second_destination = second_edge.destination();

                    assert_eq!(first_destination.vid(), vid(2));
                    assert_eq!(second_destination.vid(), vid(3));
                })),
            },
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<QueryInfoAlongEdgeFn>::new_underlying(Box::new(|info| {
                    assert_eq!(info.origin_vid(), vid(1));

                    let destination = info.destination();
                    assert_eq!(destination.vid(), vid(2));
                })),
                eid(2) => TrackCalls::<QueryInfoAlongEdgeFn>::new_underlying(Box::new(|info| {
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
                vid(1) => TrackCalls::<QueryInfoFn>::new_underlying(Box::new(|info| {
                    let local_info = info.here();
                    assert!(local_info.coerced_to_type().is_none());
                    assert_eq!(local_info.vid(), vid(1));

                    let edges: Vec<_> = local_info.edges_with_name("multiple").collect();
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
                    assert_eq!(edge, &local_info.first_edge("multiple").expect("no edge returned"));

                    // This edge is not mandatory, so it isn't returned.
                    assert!(local_info.first_mandatory_edge("multiple").is_none())
                })),
            },
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<QueryInfoAlongEdgeFn>::new_underlying(Box::new(|info| {
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
                vid(1) => TrackCalls::<QueryInfoFn>::new_underlying(Box::new(|info| {
                    let local_info = info.here();
                    assert!(local_info.coerced_to_type().is_none());
                    assert_eq!(local_info.vid(), vid(1));

                    let edges: Vec<_> = local_info.edges_with_name("successor").collect();
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
                    assert_eq!(edge, &local_info.first_edge("successor").expect("no edge returned"));

                    // This edge is not mandatory, so it isn't returned.
                    assert!(local_info.first_mandatory_edge("successor").is_none())
                })),
            },
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<QueryInfoAlongEdgeFn>::new_underlying(Box::new(|info| {
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
                vid(1) => TrackCalls::<QueryInfoFn>::new_underlying(Box::new(|info| {
                    let local_info = info.here();
                    assert!(local_info.coerced_to_type().is_none());
                    assert_eq!(local_info.vid(), vid(1));

                    let edges: Vec<_> = local_info.edges_with_name("multiple").collect();
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
                    assert_eq!(edge, &local_info.first_edge("multiple").expect("no edge returned"));

                    // This edge is not mandatory, so it isn't returned.
                    assert!(local_info.first_mandatory_edge("multiple").is_none())
                })),
            },
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<QueryInfoAlongEdgeFn>::new_underlying(Box::new(|info| {
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
}
