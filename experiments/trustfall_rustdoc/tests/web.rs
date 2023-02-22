use std::collections::BTreeMap;

use gloo_utils::format::JsValueSerdeExt;
use trustfall_rustdoc::{make_crate_info, run_query};
use trustfall_wasm::shim::JsFieldValue;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
pub fn parse_crate_info() {
    let crate_json = include_str!("./test_crate_info.json");
    make_crate_info(crate_json).expect("failed to parse");
}

#[wasm_bindgen_test]
pub fn test_run_query() {
    let crate_json = include_str!("./test_crate_info.json");
    let crate_info = make_crate_info(crate_json).expect("failed to parse");

    let query = r#"
{
    Crate {
        crate_version @output
    }
}"#;
    let args = JsValue::from_serde(&BTreeMap::<String, JsFieldValue>::default())
        .expect("conversion failed");

    let results = run_query(&crate_info, query, args).expect("no errors");
    assert!(!results.is_empty());
}
