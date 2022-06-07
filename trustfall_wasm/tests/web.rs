use trustfall_core::ir::FieldValue;
use trustfall_wasm::{
    shim::{ReturnedContextIdAndValue, ReturnedValue},
    Schema,
};
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

#[wasm_bindgen_test]
pub fn deserialize_returned_value() {
    let value: ReturnedValue = serde_json::from_str("1").expect("could not deserialize");
    let field_value: FieldValue = value.into();
    assert_eq!(field_value, FieldValue::Int64(1));
}

#[wasm_bindgen_test]
pub fn deserialize_returned_context_id_and_int_value() {
    let ctx_and_value: ReturnedContextIdAndValue =
        serde_json::from_str(r#"{"local_id":0,"value":1}"#).expect("could not deserialize");

    assert_eq!(ctx_and_value.local_id(), 0);
    let field_value: FieldValue = ctx_and_value.value().clone().into();
    assert_eq!(field_value, FieldValue::Int64(1));
}

#[wasm_bindgen_test]
pub fn deserialize_returned_context_id_and_null_value() {
    let ctx_and_value: ReturnedContextIdAndValue =
        serde_json::from_str(r#"{"local_id":2,"value":null}"#).expect("could not deserialize");

    assert_eq!(ctx_and_value.local_id(), 2);
    let field_value: FieldValue = ctx_and_value.value().clone().into();
    assert_eq!(field_value, FieldValue::Null);
}
