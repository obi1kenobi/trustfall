pub mod adapter;
pub mod indexed_crate;

use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

use adapter::RustdocAdapter;
use indexed_crate::IndexedCrate;
use ouroboros::self_referencing;
use rustdoc_types::Crate;
use trustfall_wasm::{from_js_args, shim::JsFieldValue};
use wasm_bindgen::prelude::*;

#[self_referencing]
struct InnerCrateInfo {
    crate_info: Crate,

    #[borrows(crate_info)]
    #[covariant]
    indexed_crate: IndexedCrate<'this>,
}

#[wasm_bindgen]
pub struct CrateInfo {
    inner: InnerCrateInfo,
}

#[wasm_bindgen(js_name = "makeCrateInfo")]
pub fn make_crate_info(json_text: &str) -> Result<CrateInfo, String> {
    trustfall_wasm::util::initialize().expect("init failed");

    let crate_info = serde_json::from_str(json_text)
        .map_err(|e| format!("Failed to parse rustdoc JSON content: {e}"))?;

    let inner = InnerCrateInfoBuilder {
        crate_info,
        indexed_crate_builder: |crate_info: &Crate| IndexedCrate::new(crate_info),
    }
    .build();

    Ok(CrateInfo { inner })
}

#[wasm_bindgen(js_name = "runQuery")]
pub fn run_query(
    crate_info: &CrateInfo,
    query: &str,
    args: JsValue,
) -> Result<Vec<JsValue>, String> {
    trustfall_wasm::util::initialize().expect("init failed");

    let schema = RustdocAdapter::schema();
    let adapter = Rc::new(RefCell::new(RustdocAdapter::new(
        crate_info.inner.borrow_indexed_crate(),
        None,
    )));

    let query = trustfall_core::frontend::parse(&schema, query).map_err(|e| e.to_string())?;
    let args = from_js_args(args)?;

    let results = trustfall_core::interpreter::execution::interpret_ir(adapter, query, args)
        .map_err(|e| e.to_string())?
        .map(|row| {
            let converted_row: BTreeMap<Arc<str>, JsFieldValue> =
                row.into_iter().map(|(k, v)| (k, v.into())).collect();
            JsValue::from_serde(&converted_row).expect("serde conversion failed")
        })
        .collect();

    Ok(results)
}
