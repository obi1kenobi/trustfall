#![forbid(unsafe_code)]
#![forbid(unused_lifetimes)]
#![allow(clippy::result_large_err)] // TODO: clean this up repo-wide
#![cfg_attr(docsrs, feature(doc_notable_trait))]

#[macro_use]
extern crate maplit;

#[macro_use]
extern crate lazy_static;

pub mod frontend;
pub mod graphql_query;
pub mod interpreter;
pub mod ir;
pub mod schema;
mod util;

#[cfg(test)]
mod numbers_interpreter;

#[cfg(test)]
mod filesystem_interpreter;
