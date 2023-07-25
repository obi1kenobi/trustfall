use std::{collections::BTreeMap, sync::Arc};

use once_cell::sync::Lazy;

use super::SchemaAdapter;
use crate::{
    ir::{FieldValue, TransparentValue},
    schema::Schema,
    TryIntoStruct,
};

static SCHEMA: Lazy<Schema> = Lazy::new(|| Schema::parse(SchemaAdapter::schema_text()).unwrap());

#[test]
fn enforce_adapter_invariants() {
    let adapter = SchemaAdapter::new(&SCHEMA);
    crate::interpreter::helpers::check_adapter_invariants(&SCHEMA, adapter);
}

#[test]
fn check_vertex_type_properties() {
    let query = r#"
{
    VertexType {
        name @filter(op: "=", value: ["$name"])

        property {
            property: name @output
        }
    }
}"#;
    let args = btreemap! {
        "name".into() => "VertexType".into(),
    }
    .into();
    let adapter = Arc::new(SchemaAdapter::new(&SCHEMA));

    let indexed = crate::frontend::parse(&SCHEMA, query).expect("not a valid query");
    let rows: Vec<_> = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .collect();

    let expected_rows = [
        btreemap! {
            "property".into() => "name".into(),
        },
        btreemap! {
            "property".into() => "is_interface".into(),
        },
    ];

    assert_eq!(expected_rows.as_slice(), rows);
}

#[test]
fn check_vertex_type_properties_using_one_of() {
    let query = r#"
{
    VertexType {
        name @filter(op: "one_of", value: ["$name"]) @output

        property {
            property: name @output
        }
    }
}"#;
    let args = btreemap! {
        "name".into() => vec!["VertexType", "Property"].into(),
    }
    .into();
    let adapter = Arc::new(SchemaAdapter::new(&SCHEMA));

    let indexed = crate::frontend::parse(&SCHEMA, query).expect("not a valid query");
    let mut rows: Vec<_> = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .collect();

    rows.sort_by(|a, b| {
        a["property"]
            .partial_cmp(&b["property"])
            .expect("to be comparable")
    });

    let expected_rows = [
        btreemap! {
            "property".into() => "is_interface".into(),
            "name".into() => "VertexType".into()
        },
        btreemap! {
            "property".into() => "name".into(),
            "name".into() => "VertexType".into()
        },
        btreemap! {
            "property".into() => "name".into(),
            "name".into() => "Property".into()
        },
        btreemap! {
            "property".into() => "type".into(),
            "name".into() => "Property".into()
        },
    ];

    assert_eq!(expected_rows.as_slice(), rows);
}

#[test]
fn check_entrypoint_target_edges() {
    let query = r#"
{
    Entrypoint {
        name @filter(op: "=", value: ["$name"])

        target {
            target: name @output

            edge {
                edge: name @output
            }
        }
    }
}"#;
    let args = btreemap! {
        "name".into() => "VertexType".into(),
    }
    .into();
    let adapter = Arc::new(SchemaAdapter::new(&SCHEMA));

    let indexed = crate::frontend::parse(&SCHEMA, query).expect("not a valid query");
    let rows: Vec<_> = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .collect();

    let expected_rows = [
        btreemap! {
            "target".into() => "VertexType".into(),
            "edge".into() => "implements".into(),
        },
        btreemap! {
            "target".into() => "VertexType".into(),
            "edge".into() => "implementer".into(),
        },
        btreemap! {
            "target".into() => "VertexType".into(),
            "edge".into() => "property".into(),
        },
        btreemap! {
            "target".into() => "VertexType".into(),
            "edge".into() => "edge".into(),
        },
    ];

    assert_eq!(expected_rows.as_slice(), rows);
}

#[test]
fn check_parameterized_edges() {
    let query = r#"
{
    Entrypoint {
        target {
            edge {
                edge: name @output

                parameter_: parameter {
                    name @output
                    type @output
                    default @output
                }
            }
        }
    }
}"#;
    let args = BTreeMap::new().into();

    #[derive(Debug, PartialOrd, Ord, PartialEq, Eq, serde::Deserialize)]
    struct Output {
        edge: String,
        parameter_name: String,
        parameter_type: String,
        parameter_default: Option<String>,
    }

    let test_schema = Schema::parse(include_str!(
        "../../../test_data/schemas/parameterized_edges.graphql"
    ))
    .unwrap();
    let adapter = Arc::new(SchemaAdapter::new(&test_schema));

    let indexed = crate::frontend::parse(&SCHEMA, query).expect("not a valid query");
    let mut rows: Vec<_> = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .map(|row| row.try_into_struct().expect("result shape did not match"))
        .collect();
    rows.sort_unstable();

    let mut expected_rows = [
        Output {
            edge: "nullable".into(),
            parameter_name: "x".into(),
            parameter_type: "Int".into(),
            parameter_default: Some(
                serde_json::to_string(&TransparentValue::from(FieldValue::NULL))
                    .expect("failed to serialize"),
            ),
        },
        Output {
            edge: "nonNullable".into(),
            parameter_name: "x".into(),
            parameter_type: "Int!".into(),
            parameter_default: None,
        },
        Output {
            edge: "nonNullableDefault".into(),
            parameter_name: "x".into(),
            parameter_type: "Int!".into(),
            parameter_default: Some(
                serde_json::to_string(&TransparentValue::from(FieldValue::Int64(5)))
                    .expect("failed to serialize"),
            ),
        },
        Output {
            edge: "string".into(),
            parameter_name: "y".into(),
            parameter_type: "String!".into(),
            parameter_default: Some(
                serde_json::to_string(&TransparentValue::from(FieldValue::String("abc".into())))
                    .expect("failed to serialize"),
            ),
        },
        Output {
            edge: "list".into(),
            parameter_name: "z".into(),
            parameter_type: "[String]!".into(),
            parameter_default: Some(
                serde_json::to_string(&TransparentValue::from(FieldValue::List(
                    vec![FieldValue::NULL, FieldValue::String("abc".into())].into(),
                )))
                .expect("failed to serialize"),
            ),
        },
    ];
    expected_rows.sort_unstable();

    similar_asserts::assert_eq!(expected_rows.as_slice(), rows);
}
