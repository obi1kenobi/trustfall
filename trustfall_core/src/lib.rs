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
mod serialization;
mod util;

pub use serialization::TryIntoStruct;

// Test-only uses. `#[doc(hidden)]` items are not part of public API
// and are not subject to semantic versioning rules.
#[cfg(any(test, feature = "__private"))]
#[doc(hidden)]
pub mod filesystem_interpreter;
#[cfg(any(test, feature = "__private"))]
#[doc(hidden)]
pub mod numbers_interpreter;
#[cfg(any(test, feature = "__private"))]
#[doc(hidden)]
pub mod test_types;
