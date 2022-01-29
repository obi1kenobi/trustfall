use wasm_bindgen::prelude::*;

#[macro_use] mod util;
pub mod adapter;

pub fn set_panic_hook() {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

// Schema
make_wasm_bindgen_struct_with_debug_clone!(Schema, trustfall_core::schema::Schema);

// Errors
make_wasm_bindgen_struct_with_debug_clone!(InvalidSchemaError, trustfall_core::schema::error::InvalidSchemaError);
make_wasm_bindgen_struct_with_debug_clone!(ParseError, trustfall_core::graphql_query::error::ParseError);
make_wasm_bindgen_struct_with_debug_clone!(ValidationError, trustfall_core::frontend::error::ValidationError);
make_wasm_bindgen_struct_with_debug_clone!(FrontendError, trustfall_core::frontend::error::FrontendError);
make_wasm_bindgen_struct_with_debug_clone!(InvalidIRQueryError, trustfall_core::ir::indexed::InvalidIRQueryError);
make_wasm_bindgen_struct_with_debug_clone!(QueryArgumentsError, trustfall_core::interpreter::error::QueryArgumentsError);

#[wasm_bindgen]
impl Schema {
    pub fn parse(input: &str) -> Result<Schema, crate::InvalidSchemaError> {
        trustfall_core::schema::Schema::parse(input).map(Schema::new).map_err(crate::InvalidSchemaError::new)
    }
}
