use std::{cell::RefCell, collections::BTreeMap, num::NonZeroUsize, path::PathBuf, sync::Arc};

use super::{ResolveEdgeInfo, ResolveInfo};
use crate::{
    interpreter::{
        execution::interpret_ir, Adapter, ContextIterator, ContextOutcomeIterator, VertexInfo,
        VertexIterator,
    },
    ir::{Eid, FieldValue, Recursive, Vid},
    numbers_interpreter::{NumbersAdapter, NumbersVertex},
    test_types::{TestIRQuery, TestIRQueryResult},
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
        Self { underlying: Some(underlying), calls: 0 }
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
    on_starting_vertices: RefCell<BTreeMap<Vid, TrackCalls<ResolveInfoFn>>>,
    on_property_resolver: RefCell<BTreeMap<Vid, TrackCalls<ResolveInfoFn>>>,
    on_edge_resolver: RefCell<BTreeMap<Eid, TrackCalls<ResolveEdgeInfoFn>>>,
    on_type_coercion: RefCell<BTreeMap<Vid, TrackCalls<ResolveInfoFn>>>,
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

impl<'a> Adapter<'a> for TestAdapter {
    type Vertex = NumbersVertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &crate::ir::EdgeParameters,
        resolve_info: &super::ResolveInfo,
    ) -> VertexIterator<'a, Self::Vertex> {
        let mut map_ref = self.on_starting_vertices.borrow_mut();
        if let Some(x) = map_ref.get_mut(&resolve_info.current_vid) {
            x.call(resolve_info);
        }
        drop(map_ref);
        self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info)
    }

    fn resolve_property(
        &self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &super::ResolveInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
        let mut map_ref = self.on_property_resolver.borrow_mut();
        if let Some(x) = map_ref.get_mut(&resolve_info.current_vid) {
            x.call(resolve_info);
        }
        drop(map_ref);
        self.inner.resolve_property(contexts, type_name, property_name, resolve_info)
    }

    fn resolve_neighbors(
        &self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &crate::ir::EdgeParameters,
        resolve_info: &super::ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>> {
        let mut map_ref = self.on_edge_resolver.borrow_mut();
        if let Some(x) = map_ref.get_mut(&resolve_info.eid()) {
            x.call(resolve_info);
        }
        drop(map_ref);
        self.inner.resolve_neighbors(contexts, type_name, edge_name, parameters, resolve_info)
    }

    fn resolve_coercion(
        &self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &super::ResolveInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
        let mut map_ref = self.on_type_coercion.borrow_mut();
        if let Some(x) = map_ref.get_mut(&resolve_info.current_vid) {
            x.call(resolve_info);
        }
        drop(map_ref);
        self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
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

fn run_query<A: Adapter<'static> + 'static>(adapter: A, input_name: &str) -> Arc<A> {
    let input = get_ir_for_named_query(input_name);
    let adapter = Arc::from(adapter);
    let _ = interpret_ir(
        adapter.clone(),
        Arc::new(input.ir_query.try_into().unwrap()),
        Arc::new(
            input.arguments.iter().map(|(k, v)| (Arc::from(k.to_owned()), v.clone())).collect(),
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
        }
        .into(),
        ..Default::default()
    };

    let adapter = run_query(adapter, input_name);
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
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

                let mandatory_edges: Vec<_> = info.mandatory_edges_with_name("successor").collect();
                assert_eq!(edges, mandatory_edges);

                // The "first_*" methods produce the same outcome as the iterator-based methods.
                assert_eq!(first_edge, &info.first_edge("successor").unwrap());
                assert_eq!(first_edge, &info.first_mandatory_edge("successor").unwrap());

                let first_destination = first_edge.destination();
                let second_destination = second_edge.destination();

                assert_eq!(first_destination.vid(), vid(2));
                assert_eq!(second_destination.vid(), vid(3));
            })),
        }
        .into(),
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
        }
        .into(),
        ..Default::default()
    };

    let adapter = run_query(adapter, input_name);
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
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
        }
        .into(),
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                assert_eq!(info.origin_vid(), vid(1));

                let destination = info.destination();
                assert_eq!(destination.vid(), vid(2));
            })),
        }
        .into(),
        ..Default::default()
    };

    let adapter = run_query(adapter, input_name);
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
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
        }.into(),
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                assert_eq!(info.origin_vid(), vid(1));

                let destination = info.destination();
                assert_eq!(destination.vid(), vid(2));
            })),
        }.into(),
        ..Default::default()
    };

    let adapter = run_query(adapter, input_name);
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 3); // depth 3 recursion
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
        }
        .into(),
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                assert_eq!(info.origin_vid(), vid(1));

                let destination = info.destination();
                assert_eq!(destination.vid(), vid(2));
            })),
        }
        .into(),
        ..Default::default()
    };

    let adapter = run_query(adapter, input_name);
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
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
        }
        .into(),
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
        }
        .into(),
        on_type_coercion: btreemap! {
            vid(2) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                assert_eq!(Some("Prime"), info.coerced_to_type().map(|x| x.as_ref()));
                assert_eq!(info.vid(), vid(2));
            })),
        }
        .into(),
        ..Default::default()
    };

    let adapter = run_query(adapter, input_name);
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);

    // it's inside a @fold with 7 elements
    assert_eq!(adapter.on_type_coercion.borrow()[&vid(2)].calls, 7);
}

#[test]
fn recurse_then_required_edge_depth_one() {
    let input_name = "recurse_then_required_edge_depth_one";

    let adapter = TestAdapter {
        on_starting_vertices: btreemap! {
            vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                assert_eq!(vid(1), info.vid());
                assert!(info.coerced_to_type().is_none());

                let edge_info = info.first_edge("successor").expect("no 'successor' edge info");
                let neighbor = edge_info.destination();

                assert_eq!(vid(2), neighbor.vid());

                // This edge is mandatory, since the recursion is `depth: 1`.
                // The edge is recursively traversed only once, i.e. the selected vertices
                // are the starting vertex and the neighbors, so the mandatory edge is binding
                // to all neighboring vertices.
                let next_edge_info = neighbor.first_mandatory_edge("predecessor").expect("no 'predecessor' edge info");
                let next_neighbor = next_edge_info.destination();
                assert_eq!(eid(2), next_edge_info.eid());
                assert_eq!(vid(3), next_neighbor.vid());
            })),
        }.into(),
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                let destination = info.destination();
                assert_eq!(vid(2), destination.vid());
                assert!(destination.coerced_to_type().is_none());

                // This edge is mandatory, since the recursion is `depth: 1`.
                // The edge is recursively traversed only once, i.e. the selected vertices
                // are the starting vertex and the neighbors, so the mandatory edge is binding
                // to all neighboring vertices.
                let edge_info = destination.first_mandatory_edge("predecessor").expect("no 'predecessor' edge info");
                let neighbor = edge_info.destination();
                assert_eq!(eid(2), edge_info.eid());
                assert_eq!(vid(3), neighbor.vid());
            })),
        }.into(),
        ..Default::default()
    };

    let adapter = run_query(adapter, input_name);
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
}

#[test]
fn recurse_then_nested_required_edge_depth_two() {
    let input_name = "recurse_then_nested_required_edge_depth_two";

    let adapter = TestAdapter {
        on_starting_vertices: btreemap! {
            vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                assert_eq!(vid(1), info.vid());
                assert!(info.coerced_to_type().is_none());

                let edge_info = info.first_edge("successor").expect("no 'successor' edge info");
                let neighbor = edge_info.destination();

                assert_eq!(vid(2), neighbor.vid());

                // The `predecessor` edge *isn't* mandatory here. The `successor` edge from
                // the prior step is recursively traversed up to 2 times, and the `predecessor`
                // edge is only subsequently resolved.
                //
                // The "middle" vertices in the recursion are not required to include
                // the `predecessor` edge, and including it here would be a footgun.
                assert_eq!(None, neighbor.first_mandatory_edge("predecessor"));
            })),
        }.into(),
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                let destination = info.destination();
                assert_eq!(vid(2), destination.vid());
                assert!(destination.coerced_to_type().is_none());

                // The `predecessor` edge *isn't* mandatory here. The `successor` edge from
                // the prior step is recursively traversed up to 2 times, and the `predecessor`
                // edge is only subsequently resolved.
                //
                // The "middle" vertices in the recursion are not required to include
                // the `predecessor` edge, and including it here would be a footgun.
                assert_eq!(None, destination.first_mandatory_edge("predecessor"));
            })),
            eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                let destination = info.destination();
                assert_eq!(vid(3), destination.vid());
                assert!(destination.coerced_to_type().is_none());

                // Now the edge is mandatory, because we've already finished resolving
                // the `@recurse` in a prior step.
                let edge_info = destination.first_mandatory_edge("predecessor").expect("no 'predecessor' edge info");
                let neighbor = edge_info.destination();
                assert_eq!(eid(3), edge_info.eid());
                assert_eq!(vid(4), neighbor.vid());
            })),
        }.into(),
        ..Default::default()
    };

    let adapter = run_query(adapter, input_name);
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 2); // depth 2 recursion
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
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

                    assert_eq!(None, info.statically_required_property("name"));
                    assert_eq!(
                        Some(CandidateValue::Single(&FieldValue::Int64(3))),
                        info.statically_required_property("value"),
                    );
                })),
            }
            .into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
    }

    #[test]
    fn typename_filter() {
        let input_name = "typename_filter";

        let adapter = TestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert!(info.coerced_to_type().is_none());
                    assert_eq!(vid(1), info.vid());

                    assert_eq!(None, info.statically_required_property("value"));
                    assert_eq!(
                        Some(CandidateValue::Single(&FieldValue::String("Prime".into()))),
                        info.statically_required_property("__typename"),
                    );
                })),
            }
            .into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
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
                        info.statically_required_property("name"),
                    );
                })),
            }
            .into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
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
                        info.statically_required_property("value"),
                    );
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
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
                        info.statically_required_property("value"),
                    );
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
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
                        neighbor.statically_required_property("value"),
                    );
                })),
            }.into(),
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
                        neighbor.statically_required_property("value"),
                    );
                }))
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
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
                        neighbor.statically_required_property("value"),
                    );
                })),
            }.into(),
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
                        neighbor.statically_required_property("value"),
                    );
                }))
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
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
                        neighbor.statically_required_property("value"),
                    );
                })),
            }
            .into(),
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
                        destination.statically_required_property("value"),
                    );
                })),
            }
            .into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
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
                    assert_eq!(None, neighbor.statically_required_property("value"));
                })),
            }
            .into(),
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
                    assert_eq!(None, destination.statically_required_property("value"));
                })),
            }
            .into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 2);
    }

    #[test]
    fn optional_with_nested_filter_semantics() {
        let input_name = "optional_with_nested_filter_semantics";

        let adapter = TestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(vid(1), info.vid());
                    assert!(info.coerced_to_type().is_none());

                    let edge_info = info.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let neighbor = edge_info.destination();

                    assert_eq!(vid(2), neighbor.vid());

                    // This value actually *isn't* statically known, because
                    // for the purposes of the encompassing `@optional` it's important
                    // to know whether *any* edges exist, even if none will match the filter.
                    //
                    // Including the filter's value here would be a footgun.
                    // In the absence of a way for the resolver to indicate
                    // "no matching edges existed, but some other edges were present"
                    // the safest thing to do is to return `None` here.
                    assert_eq!(None, neighbor.statically_required_property("value"));
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    let destination = info.destination();
                    assert_eq!(vid(2), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    // This value actually *isn't* statically known, because
                    // for the purposes of the encompassing `@optional` it's important
                    // to know whether *any* edges exist, even if none will match the filter.
                    //
                    // Including the filter's value here would be a footgun.
                    // In the absence of a way for the resolver to indicate
                    // "no matching edges existed, but some other edges were present"
                    // the safest thing to do is to return `None` here.
                    assert_eq!(None, destination.statically_required_property("value"));
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
    }

    #[test]
    fn optional_with_nested_required_edge_semantics() {
        let input_name = "optional_with_nested_required_edge_semantics";

        let adapter = TestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(vid(1), info.vid());
                    assert!(info.coerced_to_type().is_none());

                    let edge_info = info.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let neighbor = edge_info.destination();

                    assert_eq!(vid(2), neighbor.vid());

                    // The nested `predecessor` edge isn't actually mandatory here, because
                    // for the purposes of the encompassing `@optional` it's important
                    // to know whether *any* of the outer edges exist, even if none of
                    // the resolved vertices will satisfy the subsequent mandatory edge.
                    //
                    // Showing the nested edge as mandatory here would be a footgun.
                    // In the absence of a way for the resolver to indicate
                    // "no matching edges existed, but some other edges were present"
                    // the safest thing to do is to return `None` here.
                    assert_eq!(None, neighbor.first_mandatory_edge("predecessor"));

                    // However, it's fine to report that the edge is used as non-mandatory.
                    assert!(neighbor.first_edge("predecessor").is_some());
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    let destination = info.destination();
                    assert_eq!(vid(2), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    // The nested `predecessor` edge isn't actually mandatory here, because
                    // for the purposes of the encompassing `@optional` it's important
                    // to know whether *any* of the outer edges exist, even if none of
                    // the resolved vertices will satisfy the subsequent mandatory edge.
                    //
                    // Showing the nested edge as mandatory here would be a footgun.
                    // In the absence of a way for the resolver to indicate
                    // "no matching edges existed, but some other edges were present"
                    // the safest thing to do is to return `None` here.
                    assert_eq!(None, destination.first_mandatory_edge("predecessor"));

                    // However, it's fine to report that the edge is used as non-mandatory.
                    assert!(destination.first_edge("predecessor").is_some());
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
    }

    #[test]
    fn optional_with_nested_edge_and_filter() {
        let input_name = "optional_with_nested_edge_and_filter";

        let adapter = TestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(vid(1), info.vid());
                    assert!(info.coerced_to_type().is_none());

                    let edge_info = info.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let neighbor = edge_info.destination();
                    assert_eq!(vid(2), neighbor.vid());

                    let next_edge_info = neighbor.first_edge("successor").expect("no 'successor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value actually *isn't* statically known, because
                    // for the purposes of the encompassing `@optional` it's important
                    // to know whether *any* edges exist, even if none will match the filter.
                    //
                    // Including the filter's value here would be a footgun.
                    // In the absence of a way for the resolver to indicate
                    // "no matching edges existed, but some other edges were present"
                    // the safest thing to do is to return `None` here.
                    assert_eq!(None, next_neighbor.statically_required_property("value"));
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    let destination = info.destination();
                    assert_eq!(vid(2), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    let next_edge_info = destination.first_edge("successor").expect("no 'successor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value actually *isn't* statically known, because
                    // for the purposes of the encompassing `@optional` it's important
                    // to know whether *any* edges exist, even if none will match the filter.
                    //
                    // Including the filter's value here would be a footgun.
                    // In the absence of a way for the resolver to indicate
                    // "no matching edges existed, but some other edges were present"
                    // the safest thing to do is to return `None` here.
                    assert_eq!(None, next_neighbor.statically_required_property("value"));
                })),
                eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    let destination = info.destination();
                    assert_eq!(vid(3), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    // Here the value *is* statically known, since the `@optional`
                    // has already been resolved in a prior step.
                    assert_eq!(
                        Some(CandidateValue::Range(Range::with_start(Bound::Excluded(&FieldValue::Int64(1)), true)),),
                        destination.statically_required_property("value"),
                    );
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
    }

    #[test]
    fn fold_with_nested_filter() {
        let input_name = "fold_with_nested_filter";

        let adapter = TestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(vid(1), info.vid());
                    assert!(info.coerced_to_type().is_none());

                    let edge_info = info.first_edge("successor").expect("no 'successor' edge info");
                    let neighbor = edge_info.destination();
                    assert_eq!(vid(2), neighbor.vid());

                    let next_edge_info = neighbor.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value actually *isn't* statically known here, because
                    // for the purposes of the edge being resolved, no values for that property
                    // can possibly cause the *currently-resolved edge* to be discarded.
                    //
                    // Including the filter's value here would be a footgun.
                    assert_eq!(None, next_neighbor.statically_required_property("value"));
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    let destination = info.destination();
                    assert_eq!(vid(2), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    let next_edge_info = destination.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value actually *isn't* statically known here, because
                    // for the purposes of the edge being resolved, no values for that property
                    // can possibly cause the *currently-resolved edge* to be discarded.
                    //
                    // Including the filter's value here would be a footgun.
                    assert_eq!(None, next_neighbor.statically_required_property("value"));
                })),
                eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    let destination = info.destination();
                    assert_eq!(vid(3), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    // Here the value *is* statically known, since the property value can
                    // affect which vertices this edge is resolved to.
                    assert_eq!(
                        Some(CandidateValue::Single(&FieldValue::Int64(1))),
                        destination.statically_required_property("value"),
                    );
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
    }

    #[test]
    fn fold_with_count_filter_and_nested_filter() {
        let input_name = "fold_with_count_filter_and_nested_filter";

        let adapter = TestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                    assert_eq!(vid(1), info.vid());
                    assert!(info.coerced_to_type().is_none());

                    let edge_info = info.first_edge("successor").expect("no 'successor' edge info");
                    let neighbor = edge_info.destination();
                    assert_eq!(vid(2), neighbor.vid());

                    let next_edge_info = neighbor.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value *is* statically known here: the "fold-count-filter" around it
                    // ensures that at least one such value must exist, or else vertices
                    // from the currently-resolved edge will be discarded.
                    assert_eq!(
                        Some(CandidateValue::Single(&FieldValue::Int64(1))),
                        next_neighbor.statically_required_property("value"),
                    );
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    let destination = info.destination();
                    assert_eq!(vid(2), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    let next_edge_info = destination.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value *is* statically known here: the "fold-count-filter" around it
                    // ensures that at least one such value must exist, or else vertices
                    // from the currently-resolved edge will be discarded.
                    assert_eq!(
                        Some(CandidateValue::Single(&FieldValue::Int64(1))),
                        next_neighbor.statically_required_property("value"),
                    );
                })),
                eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                    let destination = info.destination();
                    assert_eq!(vid(3), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    // Here the value is also statically known, since the property is local
                    // to the edge being resolved.
                    assert_eq!(
                        Some(CandidateValue::Single(&FieldValue::Int64(1))),
                        destination.statically_required_property("value"),
                    );
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
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
    type ResolveInfoFn = Box<dyn FnMut(&NumbersAdapter, CtxIter, &ResolveInfo) -> CtxIter>;
    type ResolveEdgeInfoFn = Box<dyn FnMut(&NumbersAdapter, CtxIter, &ResolveEdgeInfo) -> CtxIter>;

    #[derive(Default)]
    struct TrackCalls<F> {
        underlying: F,
        calls: usize,
    }

    impl<F> TrackCalls<F> {
        fn new_underlying(underlying: F) -> Self {
            Self { underlying, calls: 0 }
        }
    }

    impl TrackCalls<ResolveInfoFn> {
        fn call(&mut self, adapter: &NumbersAdapter, ctxs: CtxIter, info: &ResolveInfo) -> CtxIter {
            self.calls += 1;
            (self.underlying)(adapter, ctxs, info)
        }
    }

    impl TrackCalls<ResolveEdgeInfoFn> {
        fn call(
            &mut self,
            adapter: &NumbersAdapter,
            ctxs: CtxIter,
            info: &ResolveEdgeInfo,
        ) -> CtxIter {
            self.calls += 1;
            (self.underlying)(adapter, ctxs, info)
        }
    }

    struct DynamicTestAdapter {
        on_starting_vertices: RefCell<BTreeMap<Vid, TrackCalls<ResolveInfoFn>>>,
        on_property_resolver: RefCell<BTreeMap<Vid, TrackCalls<ResolveInfoFn>>>,
        on_edge_resolver: RefCell<BTreeMap<Eid, TrackCalls<ResolveEdgeInfoFn>>>,
        on_type_coercion: RefCell<BTreeMap<Vid, TrackCalls<ResolveInfoFn>>>,
        inner: NumbersAdapter,
    }

    impl DynamicTestAdapter {
        fn new() -> Self {
            Self {
                inner: NumbersAdapter::new(),
                on_starting_vertices: RefCell::new(Default::default()),
                on_property_resolver: RefCell::new(Default::default()),
                on_edge_resolver: RefCell::new(Default::default()),
                on_type_coercion: RefCell::new(Default::default()),
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
            &self,
            edge_name: &Arc<str>,
            parameters: &crate::ir::EdgeParameters,
            resolve_info: &super::ResolveInfo,
        ) -> VertexIterator<'static, Self::Vertex> {
            let mut map_ref = self.on_starting_vertices.borrow_mut();
            if let Some(x) = map_ref.get_mut(&resolve_info.current_vid) {
                // the starting vertices call doesn't have an iterator
                let _ = x.call(&self.inner, Box::new(std::iter::empty()), resolve_info);
            }
            drop(map_ref);
            self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info)
        }

        fn resolve_property(
            &self,
            mut contexts: ContextIterator<'static, Self::Vertex>,
            type_name: &Arc<str>,
            property_name: &Arc<str>,
            resolve_info: &super::ResolveInfo,
        ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
            let mut map_ref = self.on_property_resolver.borrow_mut();
            if let Some(x) = map_ref.get_mut(&resolve_info.current_vid) {
                contexts = x.call(&self.inner, contexts, resolve_info);
            }
            drop(map_ref);
            self.inner.resolve_property(contexts, type_name, property_name, resolve_info)
        }

        fn resolve_neighbors(
            &self,
            mut contexts: ContextIterator<'static, Self::Vertex>,
            type_name: &Arc<str>,
            edge_name: &Arc<str>,
            parameters: &crate::ir::EdgeParameters,
            resolve_info: &super::ResolveEdgeInfo,
        ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>>
        {
            let mut map_ref = self.on_edge_resolver.borrow_mut();
            if let Some(x) = map_ref.get_mut(&resolve_info.eid()) {
                contexts = x.call(&self.inner, contexts, resolve_info);
            }
            drop(map_ref);
            self.inner.resolve_neighbors(contexts, type_name, edge_name, parameters, resolve_info)
        }

        fn resolve_coercion(
            &self,
            mut contexts: ContextIterator<'static, Self::Vertex>,
            type_name: &Arc<str>,
            coerce_to_type: &Arc<str>,
            resolve_info: &super::ResolveInfo,
        ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
            let mut map_ref = self.on_type_coercion.borrow_mut();
            if let Some(x) = map_ref.get_mut(&resolve_info.current_vid) {
                contexts = x.call(&self.inner, contexts, resolve_info);
            }
            drop(map_ref);
            self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info)
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
                    assert_eq!(None, info.dynamically_required_property("name"));

                    let edge = info.first_edge("successor").expect("no 'successor' edge");
                    let destination = edge.destination();
                    // We haven't resolved Vid 1 yet, so this property
                    // isn't dynamically known yet.
                    assert_eq!(None, destination.dynamically_required_property("value"));

                    ctxs
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|adapter, ctxs, info| {
                    assert_eq!(eid(1), info.eid());
                    assert_eq!(vid(1), info.origin_vid());
                    assert_eq!(vid(2), info.destination_vid());

                    let destination = info.destination();

                    let expected_values = [
                        CandidateValue::Single(FieldValue::Int64(3)),
                        CandidateValue::Multiple(vec![FieldValue::Int64(3), FieldValue::Int64(4)]),
                    ];
                    let value_candidate = destination.dynamically_required_property("value");
                    Box::new(value_candidate
                        .expect("no dynamic candidate for 'value' property")
                        .resolve(adapter, ctxs)
                        .zip_longest(expected_values)
                        .map(move |data| {
                            if let EitherOrBoth::Both((ctx, value), expected_value) = data {
                                assert_eq!(expected_value, value);
                                ctx
                            } else {
                                panic!("unexpected iterator outcome: {data:?}")
                            }
                        }))
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
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
                    assert_eq!(None, destination.dynamically_required_property("value"));

                    ctxs
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|adapter, ctxs, info| {
                    assert_eq!(eid(1), info.eid());
                    assert_eq!(vid(1), info.origin_vid());
                    assert_eq!(vid(2), info.destination_vid());

                    let destination = info.destination();

                    let expected_values = [
                        CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(0)), true)),
                        CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(1)), true)),
                        CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(2)), true)),
                        CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(3)), true)),
                        CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(4)), true)),
                        CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(5)), true)),
                    ];
                    let value_candidate = destination.dynamically_required_property("value");
                    Box::new(value_candidate
                        .expect("no dynamic candidate for 'value' property")
                        .resolve(adapter, ctxs)
                        .zip_longest(expected_values)
                        .map(move |data| {
                            if let EitherOrBoth::Both((ctx, value), expected_value) = data {
                                assert_eq!(expected_value, value);
                                ctx
                            } else {
                                panic!("unexpected iterator outcome: {data:?}")
                            }
                        }))
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
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
                    assert_eq!(None, destination.dynamically_required_property("value"));

                    ctxs
                })),
            }
            .into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    assert_eq!(eid(1), info.eid());
                    assert_eq!(vid(1), info.origin_vid());
                    assert_eq!(vid(2), info.destination_vid());

                    let destination = info.destination();

                    let value_candidate = destination.dynamically_required_property("value");
                    assert_eq!(None, value_candidate);
                    ctxs
                })),
            }
            .into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 2);
    }

    #[test]
    fn filter_in_fold_using_external_tag() {
        let input_name = "filter_in_fold_using_external_tag";

        let adapter = DynamicTestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    assert!(info.coerced_to_type().is_none());
                    assert_eq!(vid(1), info.vid());

                    let edge = info.first_edge("multiple").expect("no 'multiple' edge");
                    let destination = edge.destination();
                    // We haven't resolved Vid 1 yet, so this property
                    // isn't dynamically known yet.
                    assert_eq!(None, destination.dynamically_required_property("name"));

                    ctxs
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|adapter, ctxs, info| {
                    assert_eq!(eid(1), info.eid());
                    assert_eq!(vid(1), info.origin_vid());
                    assert_eq!(vid(2), info.destination_vid());

                    let destination = info.destination();

                    let expected_values = [
                        CandidateValue::Range(Range::with_end(Bound::Excluded("two".into()), true)),
                    ];
                    let candidate = destination.dynamically_required_property("name");
                    Box::new(candidate
                        .expect("no dynamic candidate for 'value' property")
                        .resolve(adapter, ctxs)
                        .zip_longest(expected_values)
                        .map(move |data| {
                            if let EitherOrBoth::Both((ctx, value), expected_value) = data {
                                assert_eq!(expected_value, value);
                                ctx
                            } else {
                                panic!("unexpected iterator outcome: {data:?}")
                            }
                        }))
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
    }

    #[test]
    fn filter_in_nested_fold_using_external_tag() {
        let input_name = "filter_in_nested_fold_using_external_tag";

        let adapter = DynamicTestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    assert!(info.coerced_to_type().is_none());
                    assert_eq!(vid(1), info.vid());

                    let edge = info.first_edge("multiple").expect("no 'multiple' edge");
                    let destination = edge.destination();
                    // We haven't resolved Vid 1 yet, so this property
                    // isn't dynamically known yet.
                    assert_eq!(None, destination.dynamically_required_property("name"));

                    ctxs
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    assert_eq!(eid(1), info.eid());
                    assert_eq!(vid(1), info.origin_vid());
                    assert_eq!(vid(2), info.destination_vid());

                    let edge = info
                        .destination()
                        .first_edge("multiple")
                        .expect("no 'multiple' edge");
                    let destination = edge.destination();

                    // For the purposes of *this* edge, the subsequent fold's values aren't yet
                    // dynamically known: no matter their value, they can't affect the vertices
                    // that this edge resolves to.
                    assert_eq!(None, destination.dynamically_required_property("name"));

                    ctxs
                })),
                eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|adapter, ctxs, info| {
                    assert_eq!(eid(2), info.eid());
                    assert_eq!(vid(2), info.origin_vid());
                    assert_eq!(vid(3), info.destination_vid());

                    let destination = info.destination();

                    let expected_values = [
                        CandidateValue::Range(Range::with_end(Bound::Excluded("two".into()), true)),
                    ];
                    let candidate = destination.dynamically_required_property("name");
                    Box::new(candidate
                        .expect("no dynamic candidate for 'name' property")
                        .resolve(adapter, ctxs)
                        .zip_longest(expected_values)
                        .map(move |data| {
                            if let EitherOrBoth::Both((ctx, value), expected_value) = data {
                                assert_eq!(expected_value, value);
                                ctx
                            } else {
                                panic!("unexpected iterator outcome: {data:?}")
                            }
                        }))
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
    }

    #[test]
    fn fold_count_tag_explicitly_named() {
        let input_name = "fold_count_tag_explicitly_named";

        let adapter = DynamicTestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    assert!(info.coerced_to_type().is_none());
                    assert_eq!(vid(1), info.vid());

                    let edge = info.first_edge("predecessor").expect("no 'predecessor' edge");
                    let destination = edge.destination();
                    // We haven't resolved Vid 1 nor Vid 2 yet,
                    // so this property isn't dynamically known yet.
                    assert_eq!(None, destination.dynamically_required_property("value"));

                    ctxs
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|adapter, ctxs, info| {
                    assert_eq!(eid(2), info.eid());
                    assert_eq!(vid(1), info.origin_vid());
                    assert_eq!(vid(3), info.destination_vid());

                    let destination = info.destination();

                    let expected_values = [
                        CandidateValue::Single(FieldValue::Int64(1)),
                    ];
                    let candidate = destination.dynamically_required_property("value");
                    Box::new(candidate
                        .expect("no dynamic candidate for 'value' property")
                        .resolve(adapter, ctxs)
                        .zip_longest(expected_values)
                        .map(move |data| {
                            if let EitherOrBoth::Both((ctx, value), expected_value) = data {
                                assert_eq!(expected_value, value);
                                ctx
                            } else {
                                panic!("unexpected iterator outcome: {data:?}")
                            }
                        }))
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
    }

    #[test]
    fn optional_with_nested_filter_with_tag_semantics() {
        let input_name = "optional_with_nested_filter_with_tag_semantics";

        let adapter = DynamicTestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    assert_eq!(vid(1), info.vid());
                    assert!(info.coerced_to_type().is_none());

                    let edge_info = info.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let neighbor = edge_info.destination();

                    assert_eq!(vid(2), neighbor.vid());

                    // This value actually *isn't* dynamically known, because
                    // for the purposes of the encompassing `@optional` it's important
                    // to know whether *any* edges exist, even if none will match the filter.
                    //
                    // Including the filter's value here would be a footgun.
                    // In the absence of a way for the resolver to indicate
                    // "no matching edges existed, but some other edges were present"
                    // the safest thing to do is to return `None` here.
                    assert_eq!(None, neighbor.dynamically_required_property("value"));

                    ctxs
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    let destination = info.destination();
                    assert_eq!(vid(2), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    // This value actually *isn't* dynamically known, because
                    // for the purposes of the encompassing `@optional` it's important
                    // to know whether *any* edges exist, even if none will match the filter.
                    //
                    // Including the filter's value here would be a footgun.
                    // In the absence of a way for the resolver to indicate
                    // "no matching edges existed, but some other edges were present"
                    // the safest thing to do is to return `None` here.
                    assert_eq!(None, destination.dynamically_required_property("value"));

                    ctxs
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
    }

    #[test]
    fn optional_with_nested_edge_with_filter_and_tag() {
        let input_name = "optional_with_nested_edge_with_filter_and_tag";

        let adapter = DynamicTestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    assert_eq!(vid(1), info.vid());
                    assert!(info.coerced_to_type().is_none());

                    let edge_info = info.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let neighbor = edge_info.destination();
                    assert_eq!(vid(2), neighbor.vid());

                    let next_edge_info = neighbor.first_edge("successor").expect("no 'successor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value actually *isn't* dynamically known, because
                    // for the purposes of the encompassing `@optional` it's important
                    // to know whether *any* edges exist, even if none will match the filter.
                    //
                    // Including the filter's value here would be a footgun.
                    // In the absence of a way for the resolver to indicate
                    // "no matching edges existed, but some other edges were present"
                    // the safest thing to do is to return `None` here.
                    assert_eq!(None, next_neighbor.dynamically_required_property("value"));

                    ctxs
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    let destination = info.destination();
                    assert_eq!(vid(2), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    let next_edge_info = destination.first_edge("successor").expect("no 'successor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value actually *isn't* dynamically known, because
                    // for the purposes of the encompassing `@optional` it's important
                    // to know whether *any* edges exist, even if none will match the filter.
                    //
                    // Including the filter's value here would be a footgun.
                    // In the absence of a way for the resolver to indicate
                    // "no matching edges existed, but some other edges were present"
                    // the safest thing to do is to return `None` here.
                    assert_eq!(None, next_neighbor.statically_required_property("value"));

                    ctxs
                })),
                eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|adapter, ctxs, info| {
                    let destination = info.destination();
                    assert_eq!(vid(3), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    // Here the value *is* dynamically known, since the `@optional`
                    // has already been resolved in a prior step.
                    let expected_values = [
                        CandidateValue::Range(Range::with_start(Bound::Excluded(FieldValue::Int64(1)), true)),
                    ];
                    let candidate = destination.dynamically_required_property("value");
                    Box::new(candidate
                        .expect("no dynamic candidate for 'value' property")
                        .resolve(adapter, ctxs)
                        .zip_longest(expected_values)
                        .map(move |data| {
                            if let EitherOrBoth::Both((ctx, value), expected_value) = data {
                                assert_eq!(expected_value, value);
                                ctx
                            } else {
                                panic!("unexpected iterator outcome: {data:?}")
                            }
                        }))
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
    }

    #[test]
    fn fold_with_nested_filter_and_tag() {
        let input_name = "fold_with_nested_filter_and_tag";

        let adapter = DynamicTestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    assert_eq!(vid(1), info.vid());
                    assert!(info.coerced_to_type().is_none());

                    let edge_info = info.first_edge("successor").expect("no 'successor' edge info");
                    let neighbor = edge_info.destination();
                    assert_eq!(vid(2), neighbor.vid());

                    let next_edge_info = neighbor.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value actually *isn't* dynamically known here, because
                    // for the purposes of the edge being resolved, no values for that property
                    // can possibly cause the *currently-resolved edge* to be discarded.
                    //
                    // Including the filter's value here would be a footgun.
                    assert_eq!(None, next_neighbor.dynamically_required_property("value"));

                    ctxs
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    let destination = info.destination();
                    assert_eq!(vid(2), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    let next_edge_info = destination.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value actually *isn't* dynamically known here, because
                    // for the purposes of the edge being resolved, no values for that property
                    // can possibly cause the *currently-resolved edge* to be discarded.
                    //
                    // Including the filter's value here would be a footgun.
                    assert_eq!(None, next_neighbor.dynamically_required_property("value"));

                    ctxs
                })),
                eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|adapter, ctxs, info| {
                    let destination = info.destination();
                    assert_eq!(vid(3), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    // Here the value *is* dynamically known, since the property value can
                    // affect which vertices this edge is resolved to.
                    let expected_values = [
                        CandidateValue::Single(FieldValue::Int64(1)),
                    ];
                    let candidate = destination.dynamically_required_property("value");
                    Box::new(candidate
                        .expect("no dynamic candidate for 'value' property")
                        .resolve(adapter, ctxs)
                        .zip_longest(expected_values)
                        .map(move |data| {
                            if let EitherOrBoth::Both((ctx, value), expected_value) = data {
                                assert_eq!(expected_value, value);
                                ctx
                            } else {
                                panic!("unexpected iterator outcome: {data:?}")
                            }
                        }))
                })),
            }.into(),
            ..Default::default()
        };
        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
    }

    #[test]
    fn fold_with_count_filter_and_nested_filter_with_tag() {
        let input_name = "fold_with_count_filter_and_nested_filter_with_tag";

        let adapter = DynamicTestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    assert_eq!(vid(1), info.vid());
                    assert!(info.coerced_to_type().is_none());

                    let edge_info = info.first_edge("successor").expect("no 'successor' edge info");
                    let neighbor = edge_info.destination();
                    assert_eq!(vid(2), neighbor.vid());

                    let next_edge_info = neighbor.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value is not yet dynamically known: Vid 1 hasn't been resolved yet,
                    // and the dynamic candidate value depends on Vid 1.
                    assert_eq!(None, next_neighbor.dynamically_required_property("value"));

                    ctxs
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|adapter, ctxs, info| {
                    let destination = info.destination();
                    assert_eq!(vid(2), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    let next_edge_info = destination.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value *is* dynamically known here: the "fold-count-filter" around it
                    // ensures that at least one such value must exist, or else vertices
                    // from the currently-resolved edge will be discarded.
                    let expected_values = [
                        CandidateValue::Single(FieldValue::Int64(1)),
                    ];
                    let candidate = next_neighbor.dynamically_required_property("value");
                    Box::new(candidate
                        .expect("no dynamic candidate for 'value' property")
                        .resolve(adapter, ctxs)
                        .zip_longest(expected_values)
                        .map(move |data| {
                            if let EitherOrBoth::Both((ctx, value), expected_value) = data {
                                assert_eq!(expected_value, value);
                                ctx
                            } else {
                                panic!("unexpected iterator outcome: {data:?}")
                            }
                        }))
                })),
                eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|adapter, ctxs, info| {
                    let destination = info.destination();
                    assert_eq!(vid(3), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    // Here the value is also dynamically known, since the property is local
                    // to the edge being resolved.
                    let expected_values = [
                        CandidateValue::Single(FieldValue::Int64(1)),
                    ];
                    let candidate = destination.dynamically_required_property("value");
                    Box::new(candidate
                        .expect("no dynamic candidate for 'value' property")
                        .resolve(adapter, ctxs)
                        .zip_longest(expected_values)
                        .map(move |data| {
                            if let EitherOrBoth::Both((ctx, value), expected_value) = data {
                                assert_eq!(expected_value, value);
                                ctx
                            } else {
                                panic!("unexpected iterator outcome: {data:?}")
                            }
                        }))
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
    }

    /// This test pins down the *current implementation's behavior*.
    /// It is possible for an improved future implementation to do better on this test.
    ///
    /// The current implementation cannot use tagged values to determine whether a fold
    /// is required to have at least one element or not. In such cases, it will act
    /// conservatively and return `None` when queried about known values.
    #[test]
    fn fold_with_both_count_and_nested_filter_dependent_on_tag() {
        let input_name = "fold_with_both_count_and_nested_filter_dependent_on_tag";

        let adapter = DynamicTestAdapter {
            on_starting_vertices: btreemap! {
                vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    assert_eq!(vid(1), info.vid());
                    assert!(info.coerced_to_type().is_none());

                    let edge_info = info.first_edge("successor").expect("no 'successor' edge info");
                    let neighbor = edge_info.destination();
                    assert_eq!(vid(2), neighbor.vid());

                    let next_edge_info = neighbor.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This value is not yet dynamically known: Vid 1 hasn't been resolved yet,
                    // and the dynamic candidate value depends on Vid 1.
                    assert_eq!(None, next_neighbor.dynamically_required_property("value"));

                    ctxs
                })),
            }.into(),
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|_, ctxs, info| {
                    let destination = info.destination();
                    assert_eq!(vid(2), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    let next_edge_info = destination.first_edge("predecessor").expect("no 'predecessor' edge info");
                    let next_neighbor = next_edge_info.destination();
                    assert_eq!(vid(3), next_neighbor.vid());

                    // This is where the current implementation is more conservative than
                    // strictly necessary:
                    //
                    // If the implementation were able to read the tagged value used in
                    // the fold-count-filter, it could determine that the fold is required
                    // to have at least one element and subsequently determine the
                    // dynamically-known value of `CandidateValue::Single(FieldValue::Int64(1))`
                    // for the `value` property below.
                    //
                    // If some instances of the tagged value are insufficient to require
                    // at least one folded element, those contexts would get
                    // `CandidateValue::All` for the dynamically-known value read below.
                    //
                    // However, the current implementation is not smart enough to do this,
                    // and instead conservatively returns `None` instead.
                    // This test pins down that behavior.
                    //
                    // TODO: consider making the hints analysis smarter here,
                    //       as described above
                    assert_eq!(None, next_neighbor.dynamically_required_property("value"));

                    ctxs
                })),
                eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|adapter, ctxs, info| {
                    let destination = info.destination();
                    assert_eq!(vid(3), destination.vid());
                    assert!(destination.coerced_to_type().is_none());

                    // Here the value is also dynamically known, since the property is local
                    // to the edge being resolved.
                    let expected_values = [
                        CandidateValue::Single(FieldValue::Int64(1)),
                    ];
                    let candidate = destination.dynamically_required_property("value");
                    Box::new(candidate
                        .expect("no dynamic candidate for 'value' property")
                        .resolve(adapter, ctxs)
                        .zip_longest(expected_values)
                        .map(move |data| {
                            if let EitherOrBoth::Both((ctx, value), expected_value) = data {
                                assert_eq!(expected_value, value);
                                ctx
                            } else {
                                panic!("unexpected iterator outcome: {data:?}")
                            }
                        }))
                })),
            }.into(),
            ..Default::default()
        };

        let adapter = run_query(adapter, input_name);
        assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
    }
}
