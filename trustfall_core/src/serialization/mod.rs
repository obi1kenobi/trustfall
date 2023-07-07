use std::{collections::BTreeMap, sync::Arc};

use serde::de::DeserializeOwned;

use crate::ir::FieldValue;

mod deserializers;

#[cfg(test)]
mod tests;

/// Deserialize Trustfall query results or edge parameters into a Rust struct.
///
/// # Use with query results
///
/// Running a Trustfall query produces an iterator of `BTreeMap<Arc<str>, FieldValue>` outputs
/// representing the query results. These maps all have a common "shape" — the same keys and
/// the same value types — as determined by the query and schema.
///
/// This trait allows deserializing those query result maps into a dedicated struct,
/// to get you easy access to strongly-typed data instead of [`FieldValue`] enums.
///
/// ## Example
///
/// Say we ran a query like:
/// ```graphql
/// query {
///     Order {
///         item_name @output
///         quantity @output
///     }
/// }
/// ```
///
/// Each of this query's outputs contain a string named `item_name` and an integer named `quantity`.
/// This trait allows us to define an output struct type:
/// ```rust
/// #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
/// struct Output {
///     item_name: String,
///     quantity: i64,
/// }
/// ```
///
/// We can then unpack the query results into an iterator of such structs:
/// ```rust
/// # use std::{collections::BTreeMap, sync::Arc};
/// # use maplit::btreemap;
/// # use trustfall_core::ir::FieldValue;
/// #
/// # fn run_query() -> Result<Box<dyn Iterator<Item = BTreeMap<Arc<str>, FieldValue>>>, ()> {
/// #     Ok(Box::new(vec![
/// #        btreemap! {
/// #           Arc::from("item_name") => FieldValue::String("widget".to_string()),
/// #           Arc::from("quantity") => FieldValue::Int64(42),
/// #        }
/// #     ].into_iter()))
/// # }
/// #
/// # #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
/// # struct Output {
/// #     item_name: String,
/// #     quantity: i64,
/// # }
///
/// use trustfall_core::TryIntoStruct;
///
/// let results: Vec<_> = run_query()
///     .expect("bad query arguments")
///     .map(|v| v.try_into_struct().expect("struct definition did not match query result shape"))
///     .collect();
///
/// assert_eq!(
///     vec![
///         Output {
///             item_name: "widget".to_string(),
///             quantity: 42,
///         },
///     ],
///     results,
/// );
/// ```
///
/// # Use with edge parameters
///
/// Edges defined in Trustfall schemas may take parameters, for example:
/// ```graphql
/// type NewsWebsite {
///     latest_stories(count: Int!): [Story!]!
/// }
/// ```
///
/// This trait can be used to deserialize [`&EdgeParameters`](crate::ir::EdgeParameters)
/// into a struct specific to the parameters of that edge:
/// ```rust
/// #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
/// struct LatestStoriesParameters {
///     count: usize
/// }
/// ```
///
/// For example:
/// ```rust
/// # use trustfall_core::{ir::EdgeParameters, interpreter::ContextIterator};
/// #
/// # #[derive(Debug, Clone)]
/// # struct Vertex;
/// #
/// # #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
/// # struct LatestStoriesParameters {
/// #     count: usize
/// # }
///
/// use trustfall_core::TryIntoStruct;
///
/// fn resolve_latest_stories(contexts: ContextIterator<Vertex>, parameters: &EdgeParameters) {
///     let parameters: LatestStoriesParameters = parameters
///         .try_into_struct()
///         .expect("edge parameters did not match struct definition");
///     let count = parameters.count;
///
///     // then resolve the edge with the given count
/// }
/// ```
pub trait TryIntoStruct {
    type Error;

    fn try_into_struct<S: DeserializeOwned>(self) -> Result<S, Self::Error>;
}

impl TryIntoStruct for BTreeMap<Arc<str>, FieldValue> {
    type Error = deserializers::Error;

    fn try_into_struct<S: DeserializeOwned>(self) -> Result<S, deserializers::Error> {
        let deserializer = deserializers::QueryResultDeserializer::new(self);
        S::deserialize(deserializer)
    }
}

impl<'a> TryIntoStruct for &'a crate::ir::EdgeParameters {
    type Error = deserializers::Error;

    fn try_into_struct<S: DeserializeOwned>(self) -> Result<S, deserializers::Error> {
        let data = (*self.contents).clone();
        let deserializer = deserializers::QueryResultDeserializer::new(data);
        S::deserialize(deserializer)
    }
}
