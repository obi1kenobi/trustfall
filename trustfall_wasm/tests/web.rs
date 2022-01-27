use trustfall_wasm::Schema;
use wasm_bindgen_test::wasm_bindgen_test;

#[cfg(test)]
pub fn make_test_schema() -> Schema {
    let schema_text = "\
schema {
    query: RootSchemaQuery
}
directive @filter(op: String!, value: [String!]) on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD

type RootSchemaQuery {
    Number(max: Int!): [Number!]
}

type Number {
    name: String
    value: Int!

    predecessor: Number
    successor: Number!
    multiple(max: Int!): [Number!]
}";

    Schema::parse(schema_text).unwrap()
}

#[wasm_bindgen_test]
pub fn test_schema() {
    make_test_schema();
}
