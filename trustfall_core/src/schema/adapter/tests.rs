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

    let expected_rows = vec![
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

#[test]
fn enforce_adapter_invariants() {
    let adapter = SchemaAdapter::new(&SCHEMA);
    crate::interpreter::helpers::check_adapter_invariants(&SCHEMA, adapter);
}
