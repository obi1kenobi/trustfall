use common::{run_numbers_query, make_test_schema};
use trustfall_core::ir::FieldValue;
use trustfall_wasm::shim::{JsFieldValue, ReturnedContextIdAndValue};
use wasm_bindgen_test::wasm_bindgen_test;

#[macro_use]
extern crate maplit;

mod common;

// Currently failing because of the "export" keyword in iterify().

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

    assert_eq!(expected_results, actual_results);
}
