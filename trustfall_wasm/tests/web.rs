use common::{make_test_schema, run_numbers_query};
use trustfall_core::ir::FieldValue;
use trustfall_wasm::adapter::js_test_query;
use trustfall_wasm::shim::JsFieldValue;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[macro_use]
extern crate maplit;

mod common;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen(start)]
pub fn run_at_start() {
    trustfall_wasm::util::initialize().expect("initialize failed");
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

    let expected_results = [
        btreemap! {
            String::from("value") => JsFieldValue::Integer(2),
            String::from("next") => JsFieldValue::Integer(3),
        },
        btreemap! {
            String::from("value") => JsFieldValue::Integer(3),
            String::from("next") => JsFieldValue::Integer(4),
        },
        btreemap! {
            String::from("value") => JsFieldValue::Integer(5),
            String::from("next") => JsFieldValue::Integer(6),
        },
        btreemap! {
            String::from("value") => JsFieldValue::Integer(7),
            String::from("next") => JsFieldValue::Integer(8),
        },
    ];

    assert_eq!(expected_results.as_slice(), actual_results);
}
