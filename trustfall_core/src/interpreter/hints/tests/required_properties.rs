use std::{collections::BTreeMap, sync::Arc};

use maplit::btreemap;

use crate::{
    interpreter::{VertexInfo, execution::interpret_ir},
    ir::FieldValue,
};

use super::{ResolveEdgeInfoFn, ResolveInfoFn, TestAdapter, TrackCalls, eid, vid};

/// Executes the query and returns its rows and adapter for result and callback checks.
fn run_query(
    adapter: TestAdapter,
    query: &str,
    arguments: BTreeMap<Arc<str>, FieldValue>,
) -> (Vec<BTreeMap<Arc<str>, FieldValue>>, Arc<TestAdapter>) {
    let query = crate::frontend::parse(adapter.inner.schema(), query).unwrap();
    #[expect(clippy::arc_with_non_send_sync)]
    let adapter = Arc::new(adapter);
    let rows = interpret_ir(adapter.clone(), query, Arc::new(arguments)).unwrap().collect();
    (rows, adapter)
}

#[test]
fn property_used_only_by_fold_count_filter() {
    let query = r#"
{
    Two {
        value @tag(name: "bound")

        multiple(max: 3) @fold @transform(op: "count")
                         @filter(op: "=", value: ["%bound"]) @output(name: "count")
    }
}"#;
    let adapter = TestAdapter {
        on_starting_vertices: btreemap! {
            vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                let properties = info.required_properties().map(|p| p.name.to_string()).collect::<Vec<_>>();
                assert_eq!(properties, vec!["value"]);
            })),
        }
        .into(),
        on_property_resolver: btreemap! {
            vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                let properties = info.required_properties().map(|p| p.name.to_string()).collect::<Vec<_>>();
                assert_eq!(properties, vec!["value"]);
            })),
        }
        .into(),
        ..Default::default()
    };

    let (rows, adapter) = run_query(adapter, query, BTreeMap::new());
    assert_eq!(rows, vec![btreemap! { Arc::from("count") => FieldValue::Uint64(2) }]);
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
    assert!(adapter.on_property_resolver.borrow()[&vid(1)].calls > 0);
}

#[test]
fn property_imported_by_fold_from_another_vertex() {
    let query = r#"
{
    Two {
        predecessor {
            name @tag(name: "bound")
        }

        multiple(max: 3) @fold {
            name @filter(op: "!=", value: ["%bound"]) @output
        }
    }
}"#;
    let adapter = TestAdapter {
        on_starting_vertices: btreemap! {
            vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                assert!(info.required_properties().next().is_none());

                let predecessor = info.first_edge("predecessor").unwrap();
                let properties = predecessor.destination().required_properties()
                    .map(|p| p.name.to_string()).collect::<Vec<_>>();
                assert_eq!(properties, vec!["name"]);
            })),
        }
        .into(),
        on_property_resolver: btreemap! {
            vid(2) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                let properties = info.required_properties().map(|p| p.name.to_string()).collect::<Vec<_>>();
                assert_eq!(properties, vec!["name"]);
            })),
        }
        .into(),
        ..Default::default()
    };

    let (rows, adapter) = run_query(adapter, query, BTreeMap::new());
    assert_eq!(
        rows,
        vec![btreemap! {
            Arc::from("name") => FieldValue::List(vec![FieldValue::from("four"), FieldValue::from("six")].into()),
        }]
    );
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
    assert!(adapter.on_property_resolver.borrow()[&vid(2)].calls > 0);
}

#[test]
fn property_used_by_count_filter_two_folds_deeper() {
    let query = r#"
{
    Two {
        successor @fold {
            value @tag(name: "bound")

            successor @fold {
                successor @fold {
                    multiple(max: 4) @fold @transform(op: "count")
                                     @filter(op: "=", value: ["%bound"]) @output(name: "count")
                }
            }
        }
    }
}"#;
    let adapter = TestAdapter {
        on_starting_vertices: btreemap! {
            vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                assert!(info.required_properties().next().is_none());

                // Lookahead starts outside the defining component. The tagged property
                // is needed at the first fold's root, not at either intermediate fold's root.
                let outer = info.first_edge("successor").unwrap();
                let properties = outer.destination().required_properties()
                    .map(|p| p.name.to_string()).collect::<Vec<_>>();
                assert_eq!(properties, vec!["value"]);

                let middle = outer.destination().first_edge("successor").unwrap();
                assert!(middle.destination().required_properties().next().is_none());

                let inner = middle.destination().first_edge("successor").unwrap();
                assert!(inner.destination().required_properties().next().is_none());
            })),
        }
        .into(),
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::<ResolveEdgeInfoFn>::new_underlying(Box::new(|info| {
                let properties = info.destination().required_properties()
                    .map(|p| p.name.to_string()).collect::<Vec<_>>();
                assert_eq!(properties, vec!["value"]);
            })),
        }
        .into(),
        on_property_resolver: btreemap! {
            vid(2) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                let properties = info.required_properties().map(|p| p.name.to_string()).collect::<Vec<_>>();
                assert_eq!(properties, vec!["value"]);
            })),
        }
        .into(),
        ..Default::default()
    };

    let (rows, adapter) = run_query(adapter, query, BTreeMap::new());
    assert_eq!(
        rows,
        vec![btreemap! {
            Arc::from("count") => FieldValue::List(vec![
                FieldValue::List(vec![FieldValue::List(vec![FieldValue::Uint64(3)].into())].into()),
            ].into()),
        }]
    );
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
    assert!(adapter.on_property_resolver.borrow()[&vid(2)].calls > 0);
}

#[test]
fn repeated_property_uses_are_deduplicated() {
    let query = r#"
{
    Two {
        value @output @tag(name: "bound") @filter(op: ">", value: ["$zero"])

        successor {
            value @filter(op: ">", value: ["%bound"])
        }

        multiple(max: 3) @fold @transform(op: "count")
                         @filter(op: "=", value: ["%bound"])

        multiple(max: 3) @fold {
            value @filter(op: ">", value: ["%bound"])
        }
    }
}"#;
    let adapter = TestAdapter {
        on_starting_vertices: btreemap! {
            vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                let properties = info.required_properties().map(|p| p.name.to_string()).collect::<Vec<_>>();
                assert_eq!(properties, vec!["value"]);
            })),
        }
        .into(),
        ..Default::default()
    };

    let (rows, adapter) =
        run_query(adapter, query, btreemap! { Arc::from("zero") => FieldValue::Int64(0) });
    assert_eq!(rows, vec![btreemap! { Arc::from("value") => FieldValue::Int64(2) }]);
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
}

#[test]
fn fold_count_tags_do_not_require_adapter_properties() {
    let query = r#"
{
    Two {
        multiple(max: 3) @fold @transform(op: "count") @tag(name: "count")

        multiple(max: 3) @fold @transform(op: "count")
                         @filter(op: "=", value: ["%count"]) {
            value @filter(op: ">", value: ["%count"]) @output
        }
    }
}"#;
    let adapter = TestAdapter {
        on_starting_vertices: btreemap! {
            vid(1) => TrackCalls::<ResolveInfoFn>::new_underlying(Box::new(|info| {
                assert!(info.required_properties().next().is_none());

                let producer = info.first_edge("multiple").unwrap();
                assert!(producer.destination().required_properties().next().is_none());
            })),
        }
        .into(),
        ..Default::default()
    };

    let (rows, adapter) = run_query(adapter, query, BTreeMap::new());
    assert_eq!(
        rows,
        vec![btreemap! {
            Arc::from("value") => FieldValue::List(vec![FieldValue::Int64(4), FieldValue::Int64(6)].into()),
        }]
    );
    assert_eq!(adapter.on_starting_vertices.borrow()[&vid(1)].calls, 1);
}
