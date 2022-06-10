use std::{cell::RefCell, rc::Rc};

use wasm_bindgen::prelude::*;

use adapter::{AdapterShim, JsAdapter};

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
    trustfall_core::ir::indexed::InvalidIRQueryError
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

#[wasm_bindgen]
pub fn attempt(adapter: JsAdapter, query: &str) -> Result<(), String> {
    util::init().expect("failed init");

    let schema = trustfall_core::schema::Schema::parse(include_str!(
        "../../trustfall_core/src/resources/schemas/numbers.graphql"
    ))
    .unwrap();
    let query = trustfall_core::frontend::parse(&schema, query).map_err(|e| e.to_string())?;

    let wrapped_adapter = Rc::new(RefCell::new(AdapterShim::new(adapter)));

    let results_iter = trustfall_core::interpreter::execution::interpret_ir(
        wrapped_adapter,
        query,
        Default::default(),
    )
    .map_err(|e| e.to_string())?;
    for result in results_iter {
        log!("result={:?}", result);
    }

    Ok(())
}
