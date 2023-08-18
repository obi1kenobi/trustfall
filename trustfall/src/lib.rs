//! # Trustfall
//!
//! Trustfall is a query engine for querying any kind of data source, from APIs and databases
//! to any kind of files on disk — and even AI models.
//!
//! ## Try Trustfall in your browser
//!
//! The Trustfall Playground supports running queries against public data sources such as:
//! - the HackerNews REST APIs: <https://play.predr.ag/hackernews>
//! - the rustdoc JSON of top Rust crates: <https://play.predr.ag/rustdoc>
//!
//! For example,
//! [this link](https://play.predr.ag/hackernews#?f=1&q=IyBDcm9zcyBBUEkgcXVlcnkgKEFsZ29saWEgKyBGaXJlYmFzZSk6CiMgRmluZCBjb21tZW50cyBvbiBzdG9yaWVzIGFib3V0ICJvcGVuYWkuY29tIiB3aGVyZQojIHRoZSBjb21tZW50ZXIncyBiaW8gaGFzIGF0IGxlYXN0IG9uZSBHaXRIdWIgb3IgVHdpdHRlciBsaW5rCnF1ZXJ5IHsKICAjIFRoaXMgaGl0cyB0aGUgQWxnb2xpYSBzZWFyY2ggQVBJIGZvciBIYWNrZXJOZXdzLgogICMgVGhlIHN0b3JpZXMvY29tbWVudHMvdXNlcnMgZGF0YSBpcyBmcm9tIHRoZSBGaXJlYmFzZSBITiBBUEkuCiAgIyBUaGUgdHJhbnNpdGlvbiBpcyBzZWFtbGVzcyAtLSBpdCBpc24ndCB2aXNpYmxlIGZyb20gdGhlIHF1ZXJ5LgogIFNlYXJjaEJ5RGF0ZShxdWVyeTogIm9wZW5haS5jb20iKSB7CiAgICAuLi4gb24gU3RvcnkgewogICAgICAjIEFsbCBkYXRhIGZyb20gaGVyZSBvbndhcmQgaXMgZnJvbSB0aGUgRmlyZWJhc2UgQVBJLgogICAgICBzdG9yeVRpdGxlOiB0aXRsZSBAb3V0cHV0CiAgICAgIHN0b3J5TGluazogdXJsIEBvdXRwdXQKICAgICAgc3Rvcnk6IHN1Ym1pdHRlZFVybCBAb3V0cHV0CiAgICAgICAgICAgICAgICAgICAgICAgICAgQGZpbHRlcihvcDogInJlZ2V4IiwgdmFsdWU6IFsiJHNpdGVQYXR0ZXJuIl0pCgogICAgICBjb21tZW50IHsKICAgICAgICByZXBseSBAcmVjdXJzZShkZXB0aDogNSkgewogICAgICAgICAgY29tbWVudDogdGV4dFBsYWluIEBvdXRwdXQKCiAgICAgICAgICBieVVzZXIgewogICAgICAgICAgICBjb21tZW50ZXI6IGlkIEBvdXRwdXQKICAgICAgICAgICAgY29tbWVudGVyQmlvOiBhYm91dFBsYWluIEBvdXRwdXQKCiAgICAgICAgICAgICMgVGhlIHByb2ZpbGUgbXVzdCBoYXZlIGF0IGxlYXN0IG9uZQogICAgICAgICAgICAjIGxpbmsgdGhhdCBwb2ludHMgdG8gZWl0aGVyIEdpdEh1YiBvciBUd2l0dGVyLgogICAgICAgICAgICBsaW5rCiAgICAgICAgICAgICAgQGZvbGQKICAgICAgICAgICAgICBAdHJhbnNmb3JtKG9wOiAiY291bnQiKQogICAgICAgICAgICAgIEBmaWx0ZXIob3A6ICI%2BPSIsIHZhbHVlOiBbIiRtaW5Qcm9maWxlcyJdKQogICAgICAgICAgICB7CiAgICAgICAgICAgICAgY29tbWVudGVySURzOiB1cmwgQGZpbHRlcihvcDogInJlZ2V4IiwgdmFsdWU6IFsiJHNvY2lhbFBhdHRlcm4iXSkKICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICBAb3V0cHV0CiAgICAgICAgICAgIH0KICAgICAgICAgIH0KICAgICAgICB9CiAgICAgIH0KICAgIH0KICB9Cn0%3D&v=ewogICJzaXRlUGF0dGVybiI6ICJodHRwW3NdOi8vKFteLl0qXFwuKSpvcGVuYWkuY29tLy4qIiwKICAibWluUHJvZmlsZXMiOiAxLAogICJzb2NpYWxQYXR0ZXJuIjogIihnaXRodWJ8dHdpdHRlcilcXC5jb20vIgp9)
//! shows the results of the HackerNews query: "Which GitHub or Twitter
//! users are commenting on stories about OpenAI?"
//!
//! In the Playground, Trustfall is configured to run client-side as WASM, performing
//! all aspects of query processing (parsing, compilation, and execution) within the browser.
//! While this demo highlights Trustfall's ability to be embedded within a target application,
//! it is of course able to be used in a more traditional client-server context as well.
//!
//! ## Examples of querying real-world data with Trustfall
//!
//! - [HackerNews APIs](./trustfall/examples/hackernews/), including an overview of the query language
//!   and an example of querying REST APIs.
//! - [RSS/Atom feeds](./trustfall/examples/feeds/), showing how to query structured data
//!   like RSS/Atom feeds.
//! - [airport weather data (METAR)](./trustfall/examples/weather), showing how to query CSV data from
//!   aviation weather reports.
//!
//! Trustfall also powers the [`cargo-semver-checks`](https://crates.io/crates/cargo-semver-checks)
//! semantic versioning linter.
//! More details on the role Trustfall plays in that use case are available in
//! [this blog post](https://predr.ag/blog/speeding-up-rust-semver-checking-by-over-2000x/).

use std::{collections::BTreeMap, sync::Arc};

/// Components needed to implement data providers.
pub mod provider {
    pub use trustfall_core::interpreter::basic_adapter::BasicAdapter;
    pub use trustfall_core::interpreter::{
        Adapter, CandidateValue, ContextIterator, ContextOutcomeIterator, DataContext,
        DynamicallyResolvedValue, EdgeInfo, QueryInfo, Range, RequiredProperty, ResolveEdgeInfo,
        ResolveInfo, Typename, VertexInfo, VertexIterator,
    };
    pub use trustfall_core::ir::{EdgeParameters, Eid, Vid};

    // Helpers for common operations when building adapters.
    pub use trustfall_core::interpreter::helpers::{
        check_adapter_invariants, resolve_coercion_using_schema, resolve_coercion_with,
        resolve_neighbors_with, resolve_property_with, resolve_typename,
    };
    pub use trustfall_core::{accessor_property, field_property};

    // Derive macros for common vertex implementation details.
    pub use trustfall_derive::{TrustfallEnumVertex, Typename};
}

// Property values and query variables.
// Useful both for querying and for implementing data providers.
pub use trustfall_core::ir::{FieldValue, TransparentValue};

// Trustfall query schema.
pub use trustfall_core::schema::{Schema, SchemaAdapter};

// Trait for converting query results into structs.
pub use trustfall_core::TryIntoStruct;

/// Run a Trustfall query over the data provider specified by the given schema and adapter.
pub fn execute_query<'vertex>(
    schema: &Schema,
    adapter: Arc<impl provider::Adapter<'vertex> + 'vertex>,
    query: &str,
    variables: BTreeMap<impl Into<Arc<str>>, impl Into<FieldValue>>,
) -> anyhow::Result<Box<dyn Iterator<Item = BTreeMap<Arc<str>, FieldValue>> + 'vertex>> {
    let parsed_query = trustfall_core::frontend::parse(schema, query)?;
    let vars = Arc::new(variables.into_iter().map(|(k, v)| (k.into(), v.into())).collect());

    Ok(trustfall_core::interpreter::execution::interpret_ir(adapter, parsed_query, vars)?)
}
