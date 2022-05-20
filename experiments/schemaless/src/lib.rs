mod schema_inference;

#[macro_use]
extern crate maplit;

pub use crate::schema_inference::infer_schema_from_query;
