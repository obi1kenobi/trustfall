use std::sync::Arc;

use once_cell::sync::Lazy;

use super::SchemaAdapter;
use crate::schema::Schema;

static SCHEMA: Lazy<Schema> = Lazy::new(|| Schema::parse(SchemaAdapter::schema_text()).unwrap());

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

    let expected_rows = vec![
        btreemap! {
            "property".into() => "name".into(),
        },
        btreemap! {
            "property".into() => "is_interface".into(),
        },
    ];

    assert_eq!(expected_rows, rows);
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

    let expected_rows = vec![
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

    assert_eq!(expected_rows, rows);
}
