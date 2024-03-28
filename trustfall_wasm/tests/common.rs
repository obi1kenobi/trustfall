use std::{collections::BTreeMap, sync::Arc};
use wasm_bindgen::prelude::wasm_bindgen;

use trustfall_wasm::{
    adapter::{AdapterShim},
    shim::JsFieldValue,
    Schema,
};
use trustfall_wasm::adapter::JsAdapter;

pub fn run_numbers_query(
    query: &str,
    args: BTreeMap<String, JsFieldValue>,
) -> Result<Vec<BTreeMap<String, JsFieldValue>>, String> {
    trustfall_wasm::util::initialize().expect("init failed");

    let schema = trustfall_core::schema::Schema::parse(include_str!(
        "../../trustfall_core/test_data/schemas/numbers.graphql"
    ))
    .unwrap();
    let adapter = JsAdapter::new();

    let query = trustfall_core::frontend::parse(&schema, query).map_err(|e| e.to_string())?;

    #[allow(clippy::arc_with_non_send_sync)]
    let wrapped_adapter = Arc::new(AdapterShim::new(adapter));

    let results: Vec<_> = trustfall_core::interpreter::execution::interpret_ir(
        wrapped_adapter,
        query,
        Arc::new(args.into_iter().map(|(k, v)| (Arc::from(k), v.into())).collect()),
    )
    .map_err(|e| e.to_string())?
    .map(|res| res.into_iter().map(|(k, v)| (k.to_string(), v.into())).collect())
    .collect();

    Ok(results)
}

pub fn make_test_schema() -> Schema {
    let schema_text = "\
schema {
    query: RootSchemaQuery
}
directive @filter(op: String!, value: [String!]) repeatable on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD
directive @transform(op: String!) on FIELD

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
