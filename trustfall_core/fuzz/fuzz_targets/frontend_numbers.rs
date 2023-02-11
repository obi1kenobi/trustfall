#![no_main]
use libfuzzer_sys::fuzz_target;

extern crate trustfall_core;

use std::fs;
use std::path::PathBuf;

use async_graphql_parser::{parse_query, parse_schema, types::ServiceDocument};
use lazy_static::lazy_static;
use trustfall_core::{
    frontend::{error::FrontendError, parse},
    graphql_query::error::ParseError,
    schema::Schema,
};

fn get_service_doc() -> ServiceDocument {
    let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    buf.push("../test_data/schemas/numbers.graphql");
    let schema_path = buf.as_path();

    parse_schema(fs::read_to_string(schema_path).unwrap()).unwrap()
}

lazy_static! {
    static ref SCHEMA: Schema = Schema::new(get_service_doc());
}

fuzz_target!(|data: &[u8]| {
    if let Ok(query_string) = std::str::from_utf8(data) {
        if query_string.match_indices("...").count() <= 3 {
            if let Ok(document) = parse_query(query_string) {
                let result = parse(&SCHEMA, &document);
                if let Err(
                    FrontendError::OtherError(..)
                    | FrontendError::ParseError(ParseError::OtherError(..)),
                ) = result
                {
                    unreachable!()
                }
            }
        }
    }
});
