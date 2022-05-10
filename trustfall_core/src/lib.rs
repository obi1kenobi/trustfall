#![forbid(unsafe_code)]
#![forbid(unused_lifetimes)]

#[macro_use]
extern crate maplit;

#[macro_use]
extern crate lazy_static;

mod filesystem_interpreter;
mod util;

pub mod frontend;
pub mod graphql_query;
pub mod interpreter;
pub mod ir;
mod numbers_interpreter;
pub mod schema;
