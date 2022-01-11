#![feature(map_try_insert)]
#![forbid(unsafe_code)]

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
pub mod schema;
mod numbers_interpreter;
