//! Parsing Trustfall queries into Rust types,
//! which are then handed to the frontend for further processing.
pub(crate) mod directives;
pub mod error;
pub(crate) mod query;

// Test-only uses. `#[doc(hidden)]` items are not part of public API
// and are not subject to semantic versioning rules.
#[cfg(feature = "__private")]
#[doc(hidden)]
pub use query::parse_document;
