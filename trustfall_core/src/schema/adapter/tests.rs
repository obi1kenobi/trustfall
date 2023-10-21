use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

use super::SchemaAdapter;
use crate::{
    ir::{FieldValue, TransparentValue},
    schema::Schema,
    TryIntoStruct,
};

static SCHEMA: OnceLock<Schema> = OnceLock::new();

fn get_schema() -> &'static Schema {
    SCHEMA.get_or_init(|| Schema::parse(SchemaAdapter::schema_text()).unwrap())
}

#[test]
fn enforce_adapter_invariants() {
    let adapter = SchemaAdapter::new(get_schema());
    crate::interpreter::helpers::check_adapter_invariants(get_schema(), adapter);
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
    let adapter = Arc::new(SchemaAdapter::new(get_schema()));

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct Output {
        property: String,
    }

    let indexed = crate::frontend::parse(get_schema(), query).expect("not a valid query");
    let mut rows: Vec<_> = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .map(|row| row.try_into_struct().expect("incorrect result shape"))
        .collect();
    rows.sort_unstable();

    let expected_rows = [
        Output { property: "docs".into() },
        Output { property: "is_interface".into() },
        Output { property: "name".into() },
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
    let adapter = Arc::new(SchemaAdapter::new(get_schema()));

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct Output {
        name: String,
        property: String,
    }

    let indexed = crate::frontend::parse(get_schema(), query).expect("not a valid query");
    let mut rows: Vec<_> = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .map(|row| row.try_into_struct().expect("incorrect result shape"))
        .collect();
    rows.sort_unstable();

    let expected_rows = [
        Output { name: "Property".into(), property: "docs".into() },
        Output { name: "Property".into(), property: "name".into() },
        Output { name: "Property".into(), property: "type".into() },
        Output { name: "VertexType".into(), property: "docs".into() },
        Output { name: "VertexType".into(), property: "is_interface".into() },
        Output { name: "VertexType".into(), property: "name".into() },
    ];

    assert_eq!(expected_rows.as_slice(), rows);
}

#[test]
fn check_schema_then_vertex_type() {
    #[derive(Debug, PartialEq, Eq, serde::Deserialize, PartialOrd, Ord)]
    struct Output {
        name: String,
        property: String,
    }

    let query = r#"
{
    Schema {
        vertex_type {
            name @filter(op: "one_of", value: ["$name"]) @output

            property {
                property: name @output
            }
        }
    }
}"#;
    let args = btreemap! {
        "name".into() => vec!["VertexType", "Property"].into(),
    }
    .into();
    let adapter = Arc::new(SchemaAdapter::new(get_schema()));

    let indexed = crate::frontend::parse(get_schema(), query).expect("not a valid query");
    let mut rows = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .map(|x| x.try_into_struct::<Output>().expect("invalid conversion"))
        .collect::<Vec<_>>();
    rows.sort_unstable();

    assert_eq!(
        rows,
        vec![
            Output { name: "Property".to_owned(), property: "docs".to_owned() },
            Output { name: "Property".to_owned(), property: "name".to_owned() },
            Output { name: "Property".to_owned(), property: "type".to_owned() },
            Output { name: "VertexType".to_owned(), property: "docs".to_owned() },
            Output { name: "VertexType".to_owned(), property: "is_interface".to_owned() },
            Output { name: "VertexType".to_owned(), property: "name".to_owned() },
        ]
    );
}

#[test]
fn use_vertex_type_in_schema_edge_multiple_times() {
    #[derive(Debug, PartialEq, Eq, serde::Deserialize, PartialOrd, Ord)]
    struct Output {
        name: String,
        property: String,
        other_vertices: u64,
    }

    let query = r#"
{
    Schema {
        vertex_type {
            name @filter(op: "one_of", value: ["$name"]) @output @tag

            property {
                property: name @output
            }
        }
        vertex_type @fold @transform(op: "count") @output(name: "other_vertices") {
            name @filter(op: "!=", value: ["%name"])
        }
    }
}"#;
    let args = btreemap! {
        "name".into() => vec!["VertexType", "Property"].into(),
    }
    .into();
    let adapter = Arc::new(SchemaAdapter::new(get_schema()));

    let indexed = crate::frontend::parse(get_schema(), query).expect("not a valid query");
    let mut rows = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .map(|x| x.try_into_struct::<Output>().expect("invalid conversion"))
        .collect::<Vec<_>>();
    rows.sort_unstable();

    assert_eq!(
        rows,
        vec![
            Output { name: "Property".to_owned(), property: "docs".to_owned(), other_vertices: 4 },
            Output { name: "Property".to_owned(), property: "name".to_owned(), other_vertices: 4 },
            Output { name: "Property".to_owned(), property: "type".to_owned(), other_vertices: 4 },
            Output {
                name: "VertexType".to_owned(),
                property: "docs".to_owned(),
                other_vertices: 4
            },
            Output {
                name: "VertexType".to_owned(),
                property: "is_interface".to_owned(),
                other_vertices: 4
            },
            Output {
                name: "VertexType".to_owned(),
                property: "name".to_owned(),
                other_vertices: 4
            }
        ]
    );
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
    let adapter = Arc::new(SchemaAdapter::new(get_schema()));

    let indexed = crate::frontend::parse(get_schema(), query).expect("not a valid query");
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
                    docs @output
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
        parameter_docs: Option<String>,
        parameter_type: String,
        parameter_default: Option<String>,
    }

    let test_schema =
        Schema::parse(include_str!("../../../test_data/schemas/parameterized_edges.graphql"))
            .unwrap();
    let adapter = Arc::new(SchemaAdapter::new(&test_schema));

    let indexed = crate::frontend::parse(get_schema(), query).expect("not a valid query");
    let mut rows: Vec<_> = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .map(|row| row.try_into_struct().expect("result shape did not match"))
        .collect();
    rows.sort_unstable();

    let mut expected_rows = [
        Output {
            edge: "nullable".into(),
            parameter_name: "x".into(),
            parameter_docs: None,
            parameter_type: "Int".into(),
            parameter_default: Some(
                serde_json::to_string(&TransparentValue::from(FieldValue::NULL))
                    .expect("failed to serialize"),
            ),
        },
        Output {
            edge: "nonNullable".into(),
            parameter_name: "x".into(),
            parameter_docs: None,
            parameter_type: "Int!".into(),
            parameter_default: None,
        },
        Output {
            edge: "nonNullableDefault".into(),
            parameter_name: "x".into(),
            parameter_docs: None,
            parameter_type: "Int!".into(),
            parameter_default: Some(
                serde_json::to_string(&TransparentValue::from(FieldValue::Int64(5)))
                    .expect("failed to serialize"),
            ),
        },
        Output {
            edge: "string".into(),
            parameter_name: "y".into(),
            parameter_docs: None,
            parameter_type: "String!".into(),
            parameter_default: Some(
                serde_json::to_string(&TransparentValue::from(FieldValue::String("abc".into())))
                    .expect("failed to serialize"),
            ),
        },
        Output {
            edge: "list".into(),
            parameter_name: "z".into(),
            parameter_docs: None,
            parameter_type: "[String]!".into(),
            parameter_default: Some(
                serde_json::to_string(&TransparentValue::from(FieldValue::List(
                    vec![FieldValue::NULL, FieldValue::String("abc".into())].into(),
                )))
                .expect("failed to serialize"),
            ),
        },
        Output {
            edge: "documented".into(),
            parameter_name: "x".into(),
            parameter_docs: "Single docs line".to_string().into(),
            parameter_type: "Int".into(),
            parameter_default: Some(
                serde_json::to_string(&TransparentValue::from(FieldValue::NULL))
                    .expect("failed to serialize"),
            ),
        },
        Output {
            edge: "documented".into(),
            parameter_name: "y".into(),
            parameter_docs: "\
Multiple docs lines

With a line break in the middle
and continuous text after it."
                .to_string()
                .into(),
            parameter_type: "String".into(),
            parameter_default: Some(
                serde_json::to_string(&TransparentValue::from(FieldValue::NULL))
                    .expect("failed to serialize"),
            ),
        },
    ];
    expected_rows.sort_unstable();

    similar_asserts::assert_eq!(expected_rows.as_slice(), rows);
}

fn check_entrypoint_docs() {
    let query = r#"
{
    Entrypoint {
        name @filter(op: "=", value: ["$name"])
        docs @output
    }
}"#;
    let args = btreemap! {
        "name".into() => "Entrypoint".into(),
    }
    .into();
    let adapter = Arc::new(SchemaAdapter::new(get_schema()));

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct Output {
        docs: String,
    }

    let indexed = crate::frontend::parse(get_schema(), query).expect("not a valid query");
    let mut rows: Vec<Output> = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .map(|row| row.try_into_struct().expect("invalid result shape"))
        .collect();
    rows.sort_unstable();

    let expected_rows = [Output {
        docs: "\
The entry point edges at which querying may begin.

Corresponds to the valid edge names for the `resolve_starting_vertices()`
method for adapters over this schema."
            .into(),
    }];

    assert_eq!(expected_rows.as_slice(), rows);
}

fn check_vertex_type_docs() {
    let query = r#"
{
    VertexType {
        name @filter(op: "=", value: ["$name"])
        docs @output
    }
}"#;
    let args = btreemap! {
        "name".into() => "VertexType".into(),
    }
    .into();
    let adapter = Arc::new(SchemaAdapter::new(get_schema()));

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct Output {
        docs: String,
    }

    let indexed = crate::frontend::parse(get_schema(), query).expect("not a valid query");
    let mut rows: Vec<Output> = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .map(|row| row.try_into_struct().expect("invalid result shape"))
        .collect();
    rows.sort_unstable();

    let expected_rows = [Output { docs: "A type of vertex in a schema.".into() }];

    assert_eq!(expected_rows.as_slice(), rows);
}

fn check_property_docs() {
    let query = r#"
{
    VertexType {
        name @filter(op: "=", value: ["$name"])

        property {
            name @filter(op: "=", value: ["$property"])
            docs @output
        }
    }
}"#;
    let args = btreemap! {
        "name".into() => "VertexType".into(),
        "property".into() => "is_interface".into(),
    }
    .into();
    let adapter = Arc::new(SchemaAdapter::new(get_schema()));

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct Output {
        docs: String,
    }

    let indexed = crate::frontend::parse(get_schema(), query).expect("not a valid query");
    let mut rows: Vec<Output> = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .map(|row| row.try_into_struct().expect("invalid result shape"))
        .collect();
    rows.sort_unstable();

    let expected_rows = [Output {
        docs: "\
True if this vertex is an interface (may have subtypes),
and false otherwise."
            .into(),
    }];

    assert_eq!(expected_rows.as_slice(), rows);
}

fn check_edge_docs() {
    let query = r#"
{
    VertexType {
        name @filter(op: "=", value: ["$name"])

        edge {
            name @filter(op: "=", value: ["$edge"])
            docs @output
        }
    }
}"#;
    let args = btreemap! {
        "name".into() => "VertexType".into(),
        "edge".into() => "implementer".into(),
    }
    .into();
    let adapter = Arc::new(SchemaAdapter::new(get_schema()));

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct Output {
        docs: String,
    }

    let indexed = crate::frontend::parse(get_schema(), query).expect("not a valid query");
    let mut rows: Vec<Output> = crate::interpreter::execution::interpret_ir(adapter, indexed, args)
        .expect("execution error")
        .map(|row| row.try_into_struct().expect("invalid result shape"))
        .collect();
    rows.sort_unstable();

    let expected_rows = [Output {
        docs: "\
Subtypes of this vertex type.

If this is not an interface type, this edge is guaranteed to be empty."
            .into(),
    }];

    assert_eq!(expected_rows.as_slice(), rows);
}
