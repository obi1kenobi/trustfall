use std::{collections::BTreeMap, sync::Arc};

use maplit::btreemap;

use crate::{
    interpreter::{VertexInfo, execution::interpret_ir},
    ir::FieldValue,
};

use super::{ResolveEdgeInfoFn, ResolveInfoFn, TestAdapter, TrackCalls, eid, vid};

/// Executes a numbers-schema query to completion and returns the adapter for callback checks.
fn run_query(
    adapter: TestAdapter,
    query: &str,
    arguments: BTreeMap<Arc<str>, FieldValue>,
) -> Arc<TestAdapter> {
    let query = crate::frontend::parse(adapter.inner.schema(), query).unwrap();
    #[expect(clippy::arc_with_non_send_sync)]
    let adapter = Arc::new(adapter);
    let _ = interpret_ir(adapter.clone(), query, Arc::new(arguments)).unwrap().collect::<Vec<_>>();
    adapter
}

#[test]
fn required_path_exposes_hints() {
    let query = r#"
{
    Four {
        value @tag(name: "bound")

        successor {
            successor {
                value @filter(op: ">", value: ["$zero"])
                      @filter(op: ">", value: ["%bound"])

                successor {
                    value @output
                }
            }
        }
    }
}"#;
    let adapter = TestAdapter {
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                let required = info.destination().first_edge("successor").unwrap();
                let target = required.destination();
                assert!(required.is_mandatory());

                assert!(target.statically_required_property("value").is_some());
                assert!(target.dynamically_required_property("value").is_some());
                assert!(target.first_mandatory_edge("successor").is_some());
                assert!(target.first_edge("successor").unwrap().is_mandatory());
            })),
        }
        .into(),
        ..Default::default()
    };

    let adapter =
        run_query(adapter, query, btreemap! { Arc::from("zero") => FieldValue::Int64(0) });
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
}

#[test]
fn optional_scope_suppresses_hints_through_required_edges() {
    let query = r#"
{
    Four {
        value @tag(name: "bound")

        successor {
            successor @optional {
                value @filter(op: ">", value: ["$zero"])
                      @filter(op: ">", value: ["%bound"])

                successor {
                    value @filter(op: ">", value: ["$zero"])
                          @filter(op: ">", value: ["%bound"])

                    successor {
                        value @output
                    }
                }
            }
        }
    }
}"#;
    let adapter = TestAdapter {
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                let optional = info.destination().first_edge("successor").unwrap();
                let target = optional.destination();
                assert!(!optional.is_mandatory());

                // Pruning these neighbors would conflate an absent optional edge
                // with an existing edge whose neighbors all fail filters.
                // Only the former preserves a null row.
                assert!(target.statically_required_property("value").is_none());
                assert!(target.dynamically_required_property("value").is_none());
                assert!(target.first_mandatory_edge("successor").is_none());

                let required = target.first_edge("successor").unwrap();
                let descendant = required.destination();
                assert!(!required.is_mandatory());
                assert!(descendant.statically_required_property("value").is_none());
                assert!(descendant.dynamically_required_property("value").is_none());
                assert!(descendant.first_mandatory_edge("successor").is_none());
                assert!(!descendant.first_edge("successor").unwrap().is_mandatory());
            })),
        }
        .into(),
        on_property_resolver: btreemap! {
            vid(4) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                // At the descendant itself, its constraints bind regardless of the optional
                // edges used to reach it. The enclosing scope restricts only earlier lookahead.
                assert!(info.statically_required_property("value").is_some());
                assert!(info.dynamically_required_property("value").is_some());
                assert!(info.first_mandatory_edge("successor").is_some());
                assert!(info.first_edge("successor").unwrap().is_mandatory());
            })),
        }
        .into(),
        ..Default::default()
    };

    let adapter =
        run_query(adapter, query, btreemap! { Arc::from("zero") => FieldValue::Int64(0) });
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
    assert!(adapter.on_property_resolver.borrow()[&vid(4)].calls > 0);
}

#[test]
fn optional_scope_can_start_inside_fold() {
    let query = r#"
{
    Four {
        value @tag(name: "bound")

        successor @fold {
            successor @optional {
                value @filter(op: ">", value: ["$zero"])
                      @filter(op: ">", value: ["%bound"])

                successor {
                    value @output
                }
            }
        }
    }
}"#;
    let adapter = TestAdapter {
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                // The fold is already being resolved. The nested optional edge
                // introduces the non-binding scope. The dynamic filter uses
                // a tag from outside this component.
                let optional = info.destination().first_edge("successor").unwrap();
                let target = optional.destination();
                assert!(!optional.is_mandatory());

                assert!(target.statically_required_property("value").is_none());
                assert!(target.dynamically_required_property("value").is_none());
                assert!(target.first_mandatory_edge("successor").is_none());
                assert!(!target.first_edge("successor").unwrap().is_mandatory());
            })),
        }
        .into(),
        ..Default::default()
    };

    let adapter =
        run_query(adapter, query, btreemap! { Arc::from("zero") => FieldValue::Int64(0) });
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
}

#[test]
fn mandatory_fold_preserves_hints() {
    let query = r#"
{
    Four {
        value @tag(name: "bound")

        successor {
            successor @fold @transform(op: "count")
                      @filter(op: ">", value: ["$zero"]) {
                successor {
                    value @filter(op: ">", value: ["$zero"])
                          @filter(op: ">", value: ["%bound"])

                    successor {
                        value @output
                    }
                }
            }
        }
    }
}"#;
    let adapter = TestAdapter {
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                let fold = info.destination().first_edge("successor").unwrap();
                let required = fold.destination().first_edge("successor").unwrap();
                let target = required.destination();
                assert!(fold.is_mandatory());
                assert!(required.is_mandatory());

                assert!(target.statically_required_property("value").is_some());
                assert!(target.dynamically_required_property("value").is_some());
                assert!(target.first_mandatory_edge("successor").is_some());
                assert!(target.first_edge("successor").unwrap().is_mandatory());
            })),
            eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                assert!(info.edge().is_mandatory());
                assert!(info.destination().first_edge("successor").unwrap().is_mandatory());
            })),
        }
        .into(),
        ..Default::default()
    };

    let adapter =
        run_query(adapter, query, btreemap! { Arc::from("zero") => FieldValue::Int64(0) });
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
}

#[test]
fn optional_scope_suppresses_hints_inside_mandatory_fold() {
    let query = r#"
{
    Four {
        value @tag(name: "bound")

        successor {
            successor @optional {
                successor @fold @transform(op: "count")
                          @filter(op: ">", value: ["$zero"]) {
                    successor {
                        value @filter(op: ">", value: ["$zero"])
                              @filter(op: ">", value: ["%bound"])

                        successor {
                            value @output
                        }
                    }
                }
            }
        }
    }
}"#;
    let adapter = TestAdapter {
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                let optional = info.destination().first_edge("successor").unwrap();
                let fold = optional.destination().first_edge("successor").unwrap();
                let required = fold.destination().first_edge("successor").unwrap();
                let target = required.destination();
                assert!(!fold.is_mandatory());
                assert!(!required.is_mandatory());

                assert!(target.statically_required_property("value").is_none());
                assert!(target.dynamically_required_property("value").is_none());
                assert!(target.first_mandatory_edge("successor").is_none());
                assert!(!target.first_edge("successor").unwrap().is_mandatory());
            })),
        }
        .into(),
        ..Default::default()
    };

    let adapter =
        run_query(adapter, query, btreemap! { Arc::from("zero") => FieldValue::Int64(0) });
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
}

#[test]
fn possibly_empty_fold_suppresses_hints() {
    let query = r#"
{
    Four {
        value @tag(name: "bound")

        successor {
            successor @fold {
                successor {
                    value @filter(op: ">", value: ["$zero"])
                          @filter(op: ">", value: ["%bound"])

                    successor {
                        value @output
                    }
                }
            }
        }
    }
}"#;
    let adapter = TestAdapter {
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                let fold = info.destination().first_edge("successor").unwrap();
                let required = fold.destination().first_edge("successor").unwrap();
                let target = required.destination();
                assert!(!fold.is_mandatory());
                assert!(!required.is_mandatory());

                assert!(target.statically_required_property("value").is_none());
                assert!(target.dynamically_required_property("value").is_none());
                assert!(target.first_mandatory_edge("successor").is_none());
                assert!(!target.first_edge("successor").unwrap().is_mandatory());
            })),
            eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                // Empty folds preserve their containing row, but each element must still
                // satisfy the body once the folded edge itself is being resolved.
                assert!(!info.edge().is_mandatory());
                assert!(info.destination().first_edge("successor").unwrap().is_mandatory());
            })),
        }
        .into(),
        ..Default::default()
    };

    let adapter =
        run_query(adapter, query, btreemap! { Arc::from("zero") => FieldValue::Int64(0) });
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
}

#[test]
fn recursive_scope_suppresses_mandatory_fold() {
    let query = r#"
{
    Four {
        successor @recurse(depth: 2) {
            value @output(name: "recursive_value")

            successor @fold @transform(op: "count")
                      @filter(op: ">", value: ["$zero"]) {
                successor {
                    value @output
                }
            }
        }
    }
}"#;
    let adapter = TestAdapter {
        on_starting_vertices: btreemap! {
            vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                let recursion = info.first_edge("successor").unwrap();
                let fold = recursion.destination().first_edge("successor").unwrap();
                assert!(!fold.is_mandatory());

                // The count constraint binds after recursion finishes. Within the fold body,
                // the required edge binds because it is not part of the recursive traversal.
                assert!(fold.destination().first_edge("successor").unwrap().is_mandatory());
            })),
        }
        .into(),
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                let fold = info.destination().first_edge("successor").unwrap();
                assert!(!fold.is_mandatory());
            })),
            eid(2) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                assert!(info.edge().is_mandatory());
                assert!(info.destination().first_edge("successor").unwrap().is_mandatory());
            })),
        }
        .into(),
        on_property_resolver: btreemap! {
            vid(2) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                assert!(info.first_edge("successor").unwrap().is_mandatory());
            })),
        }
        .into(),
        ..Default::default()
    };

    let adapter =
        run_query(adapter, query, btreemap! { Arc::from("zero") => FieldValue::Int64(0) });
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 2);
    assert!(adapter.on_edge_resolver.borrow()[&eid(2)].calls > 0);
    assert!(adapter.on_property_resolver.borrow()[&vid(2)].calls > 0);
}
