use common::run_numbers_query;
use trustfall_core::ir::FieldValue;
use trustfall_wasm::{
    shim::{ReturnedContextIdAndValue, JsFieldValue},
    Schema,
};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_test::wasm_bindgen_test;

#[macro_use] extern crate maplit;

mod common;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen(start)]
pub fn run_at_start() {
    trustfall_wasm::util::init().expect("init failed");
}

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
    let value: JsFieldValue = serde_json::from_str("1").expect("could not deserialize");
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

#[wasm_bindgen(inline_js = r#"
    export function js_test_query() {

    }
"#)]
extern "C" {
    pub fn js_test_query();
}

#[wasm_bindgen_test]
pub fn test_query() {
    js_test_query();
}

#[wasm_bindgen_test]
pub fn test_execute_query_with_traversal_and_coercion() {
    let query = r#"
{
    Number(max: 10) {
        ... on Prime {
            value @output

            successor {
                next: value @output
            }
        }
    }
}"#;
    let args = Default::default();

    let actual_results = run_numbers_query(query, args).expect("query and args were not valid");

    let expected_results = vec![
        btreemap!{
            String::from("value") => JsFieldValue::Integer(2),
            String::from("next") => JsFieldValue::Integer(3),
        },
        btreemap!{
            String::from("value") => JsFieldValue::Integer(3),
            String::from("next") => JsFieldValue::Integer(4),
        },
        btreemap!{
            String::from("value") => JsFieldValue::Integer(5),
            String::from("next") => JsFieldValue::Integer(6),
        },
        btreemap!{
            String::from("value") => JsFieldValue::Integer(7),
            String::from("next") => JsFieldValue::Integer(8),
        },
    ];

    assert_eq!(expected_results, actual_results);
}
