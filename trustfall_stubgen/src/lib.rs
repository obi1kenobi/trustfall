//! # trustfall_stubgen
//!
//! Given a Trustfall schema, autogenerate a Rust adapter stub fully wired up with
//! all types, properties, and edges referenced in the schema.

#![forbid(unsafe_code)]

mod adapter_creator;
mod edges_creator;
mod entrypoints_creator;
mod properties_creator;
mod root;
mod util;

#[cfg(test)]
mod tests;

pub use root::generate_rust_stub;
