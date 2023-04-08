use std::{collections::BTreeMap, rc::Rc, sync::Arc};

use gloo_utils::format::JsValueSerdeExt;
use trustfall_core::ir::FieldValue;
use wasm_bindgen::prelude::*;

use crate::{
    adapter::{AdapterShim, JsAdapter},
    shim::{JsFieldValue, QueryResultIterator},
};

#[macro_use]
pub mod util;
pub mod adapter;
pub mod shim;

// Schema
make_wasm_bindgen_struct_with_debug_clone!(Schema, trustfall_core::schema::Schema);

// Errors
make_wasm_bindgen_struct_with_debug_clone!(
    InvalidSchemaError,
    trustfall_core::schema::error::InvalidSchemaError
);
make_wasm_bindgen_struct_with_debug_clone!(
    ParseError,
    trustfall_core::graphql_query::error::ParseError
);
make_wasm_bindgen_struct_with_debug_clone!(
    ValidationError,
    trustfall_core::frontend::error::ValidationError
);
make_wasm_bindgen_struct_with_debug_clone!(
    FrontendError,
    trustfall_core::frontend::error::FrontendError
);
make_wasm_bindgen_struct_with_debug_clone!(
    InvalidIRQueryError,
    trustfall_core::ir::InvalidIRQueryError
);
make_wasm_bindgen_struct_with_debug_clone!(
    QueryArgumentsError,
    trustfall_core::interpreter::error::QueryArgumentsError
);

#[wasm_bindgen]
impl Schema {
    pub fn parse(input: &str) -> Result<Schema, crate::InvalidSchemaError> {
        trustfall_core::schema::Schema::parse(input)
            .map(Schema::new)
            .map_err(crate::InvalidSchemaError::new)
    }
}

pub fn from_js_args(args: JsValue) -> Result<Arc<BTreeMap<Arc<str>, FieldValue>>, String> {
    // TODO: add a proper error type
    let args = args
        .into_serde::<BTreeMap<String, JsFieldValue>>()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(k, v)| (Arc::from(k), v.into()))
        .collect();

    Ok(Arc::new(args))
}

#[wasm_bindgen(js_name = "executeQuery")]
pub fn execute_query(
    schema: &Schema,
    adapter: JsAdapter,
    query: &str,
    args: JsValue,
) -> Result<QueryResultIterator, String> {
    // TODO: add a proper error type
    let args = from_js_args(args)?;

    let query = trustfall_core::frontend::parse(schema, query).map_err(|e| format!("{e}"))?;

    let wrapped_adapter = Rc::new(AdapterShim::new(adapter));

    let results_iter =
        trustfall_core::interpreter::execution::interpret_ir(wrapped_adapter, query, args)
            .map_err(|e| format!("{e}"))?;

    Ok(QueryResultIterator::new(results_iter))
}
