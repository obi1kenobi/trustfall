use std::{collections::BTreeMap, ops::Bound, sync::Arc};

use maplit::btreemap;

use crate::{
    interpreter::{
        AsVertex, CandidateValue, ContextIterator, Range, ResolveEdgeInfo, VertexInfo,
        execution::interpret_ir,
    },
    ir::FieldValue,
    numbers_interpreter::{NumbersAdapter, NumbersVertex},
};

use super::super::eid;
use super::{DynamicTestAdapter, ResolveEdgeInfoFn, TrackCalls};

/// Checks the resolved candidate for `name` in each input context, then returns
/// those contexts unchanged so ordinary query execution can continue.
struct ExpectNameCandidates(Vec<CandidateValue<FieldValue>>);

impl ResolveEdgeInfoFn for ExpectNameCandidates {
    fn call<V: AsVertex<NumbersVertex> + 'static>(
        &mut self,
        adapter: &NumbersAdapter,
        contexts: ContextIterator<'static, V>,
        info: &ResolveEdgeInfo,
    ) -> ContextIterator<'static, V> {
        let resolved = info
            .destination()
            .dynamically_required_property("name")
            .expect("expected a dynamic candidate for name")
            .resolve(adapter, contexts)
            .collect::<Vec<_>>();
        assert_eq!(
            resolved.iter().map(|(_, candidate)| candidate).collect::<Vec<_>>(),
            self.0.iter().collect::<Vec<_>>(),
        );
        Box::new(resolved.into_iter().map(|(context, _)| context))
    }
}

/// Executes the query and returns its rows and adapter for result and callback checks.
#[allow(clippy::type_complexity)]
fn run_query(
    adapter: DynamicTestAdapter<(), (), ExpectNameCandidates, ()>,
    query: &str,
    arguments: BTreeMap<Arc<str>, FieldValue>,
) -> (Vec<BTreeMap<Arc<str>, FieldValue>>, Arc<DynamicTestAdapter<(), (), ExpectNameCandidates, ()>>)
{
    let query = crate::frontend::parse(adapter.inner.schema(), query).unwrap();
    #[expect(clippy::arc_with_non_send_sync)]
    let adapter = Arc::new(adapter);
    let rows = interpret_ir(adapter.clone(), query, Arc::new(arguments)).unwrap().collect();
    (rows, adapter)
}

#[test]
fn null_ordering_tags_have_no_candidates() {
    // Numbers above twenty have no name, but their vertices exist. These tags
    // contain a real null rather than originating in an absent optional scope.
    for query in [
        r#"
{
    Number(min: 21, max: 21) {
        name @tag(name: "bound")

        successor {
            name @filter(op: "<", value: ["%bound"]) @output
        }
    }
}"#,
        r#"
{
    Number(min: 21, max: 21) {
        name @tag(name: "bound")

        successor {
            name @filter(op: "<=", value: ["%bound"]) @output
        }
    }
}"#,
        r#"
{
    Number(min: 21, max: 21) {
        name @tag(name: "bound")

        successor {
            name @filter(op: ">", value: ["%bound"]) @output
        }
    }
}"#,
        r#"
{
    Number(min: 21, max: 21) {
        name @tag(name: "bound")

        successor {
            name @filter(op: ">=", value: ["%bound"]) @output
        }
    }
}"#,
    ] {
        let adapter = DynamicTestAdapter {
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::new_underlying(ExpectNameCandidates(vec![CandidateValue::Impossible])),
            }
            .into(),
            ..Default::default()
        };
        let (rows, adapter) = run_query(adapter, query, BTreeMap::new());
        assert!(rows.is_empty(), "{query}");
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
    }
}

#[test]
fn imported_null_tags_have_no_candidates() {
    for query in [
        r#"
{
    Number(min: 21, max: 21) {
        name @tag(name: "bound")

        successor @fold {
            predecessor {
                name @filter(op: ">", value: ["%bound"]) @output
            }
        }
    }
}"#,
        r#"
{
    Number(min: 21, max: 21) {
        vowelsInName @tag(name: "bound")

        successor @fold {
            predecessor {
                name @filter(op: "one_of", value: ["%bound"]) @output
            }
        }
    }
}"#,
    ] {
        let adapter = DynamicTestAdapter {
            on_edge_resolver: btreemap! {
                eid(2) => TrackCalls::new_underlying(ExpectNameCandidates(vec![CandidateValue::Impossible])),
            }
            .into(),
            ..Default::default()
        };
        let (rows, adapter) = run_query(adapter, query, BTreeMap::new());
        assert_eq!(
            rows,
            vec![btreemap! { Arc::from("name") => FieldValue::List(vec![].into()) }],
            "{query}",
        );
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(2)].calls, 1);
    }
}

#[test]
fn nullable_list_tags_distinguish_null_from_a_list() {
    for (query, expected_candidate) in [
        (
            r#"
{
    Number(min: 21, max: 21) {
        vowelsInName @tag(name: "bound")

        successor {
            name @filter(op: "one_of", value: ["%bound"]) @output
        }
    }
}"#,
            CandidateValue::Impossible,
        ),
        (
            r#"
{
    One {
        vowelsInName @tag(name: "bound")

        successor {
            name @filter(op: "one_of", value: ["%bound"]) @output
        }
    }
}"#,
            CandidateValue::Multiple(vec![FieldValue::from("o"), FieldValue::from("e")]),
        ),
    ] {
        let adapter = DynamicTestAdapter {
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::new_underlying(ExpectNameCandidates(vec![expected_candidate])),
            }
            .into(),
            ..Default::default()
        };
        let (rows, adapter) = run_query(adapter, query, BTreeMap::new());
        assert!(rows.is_empty(), "{query}");
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
    }
}

#[test]
fn equality_and_inequality_retain_null_comparisons() {
    for (query, expected_candidate, expected_rows) in [
        (
            r#"
{
    Number(min: 21, max: 21) {
        name @tag(name: "bound")

        successor {
            name @filter(op: "=", value: ["%bound"]) @output
        }
    }
}"#,
            CandidateValue::Single(FieldValue::Null),
            vec![btreemap! { Arc::from("name") => FieldValue::Null }],
        ),
        (
            r#"
{
    Number(min: 21, max: 21) {
        name @tag(name: "bound")

        successor {
            name @filter(op: "!=", value: ["%bound"]) @output
        }
    }
}"#,
            CandidateValue::Range(Range::full_non_null()),
            vec![],
        ),
    ] {
        let adapter = DynamicTestAdapter {
            on_edge_resolver: btreemap! {
                eid(1) => TrackCalls::new_underlying(ExpectNameCandidates(vec![expected_candidate])),
            }
            .into(),
            ..Default::default()
        };
        let (rows, adapter) = run_query(adapter, query, BTreeMap::new());
        assert_eq!(rows, expected_rows, "{query}");
        assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
    }
}

#[test]
fn absent_optional_tags_preserve_static_constraints() {
    for (query, edge_id, name, expected_rows) in [
        (
            r#"
{
    Zero {
        predecessor @optional {
            name @tag(name: "bound")
        }

        successor {
            name @filter(op: ">=", value: ["%bound"])
                 @filter(op: "=", value: ["$name"]) @output
        }
    }
}"#,
            eid(2),
            "one",
            vec![btreemap! { Arc::from("name") => FieldValue::from("one") }],
        ),
        (
            r#"
{
    Zero {
        predecessor @optional {
            vowelsInName @tag(name: "allowed")
        }

        successor @fold {
            successor {
                name @filter(op: "one_of", value: ["%allowed"])
                     @filter(op: "=", value: ["$name"]) @output
            }
        }
    }
}"#,
            eid(3),
            "two",
            vec![btreemap! {
                Arc::from("name") => FieldValue::List(vec![FieldValue::from("two")].into()),
            }],
        ),
    ] {
        let adapter = DynamicTestAdapter {
            on_edge_resolver: btreemap! {
                edge_id => TrackCalls::new_underlying(ExpectNameCandidates(vec![
                    CandidateValue::Single(FieldValue::from(name)),
                ])),
            }
            .into(),
            ..Default::default()
        };
        let (rows, adapter) =
            run_query(adapter, query, btreemap! { Arc::from("name") => FieldValue::from(name) });
        assert_eq!(rows, expected_rows, "{query}");
        assert_eq!(adapter.on_edge_resolver.borrow()[&edge_id].calls, 1);
    }
}

#[test]
fn nonnull_ordering_tag_retains_candidates() {
    let query = r#"
{
    One {
        name @tag(name: "bound")

        successor {
            name @filter(op: ">", value: ["%bound"]) @output
        }
    }
}"#;
    let adapter = DynamicTestAdapter {
        on_edge_resolver: btreemap! {
            eid(1) => TrackCalls::new_underlying(ExpectNameCandidates(vec![CandidateValue::Range(Range::with_start(
                Bound::Excluded(FieldValue::from("one")),
                true,
            ))])),
        }
        .into(),
        ..Default::default()
    };
    let (rows, adapter) = run_query(adapter, query, BTreeMap::new());
    assert_eq!(rows, vec![btreemap! { Arc::from("name") => FieldValue::from("two") }]);
    assert_eq!(adapter.on_edge_resolver.borrow()[&eid(1)].calls, 1);
}
