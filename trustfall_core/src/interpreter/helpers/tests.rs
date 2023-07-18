use std::fmt::Debug;

use crate::{
    interpreter::{helpers::resolve_typename, DataContext, Typename},
    ir::FieldValue,
    schema::Schema,
};

#[test]
fn typename_resolved_statically() {
    #[derive(Debug, Clone)]
    enum Vertex {
        Variant,
    }

    impl Typename for Vertex {
        fn typename(&self) -> &'static str {
            unreachable!("typename() was called, so __typename was not resolved statically")
        }
    }

    let schema = Schema::parse(
        "\
schema {
    query: RootSchemaQuery
}
directive @filter(op: String!, value: [String!]) on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD
directive @transform(op: String!) on FIELD

type RootSchemaQuery {
    Vertex: Vertex!
}

type Vertex {
    field: Int
}",
    )
    .expect("failed to parse schema");
    let contexts = Box::new(std::iter::once(DataContext::new(Some(Vertex::Variant))));

    let outputs: Vec<_> = resolve_typename(contexts, &schema, "Vertex")
        .map(|(_ctx, value)| value)
        .collect();

    assert_eq!(vec![FieldValue::from("Vertex")], outputs);
}
