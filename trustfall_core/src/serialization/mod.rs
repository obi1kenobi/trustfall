use std::{collections::BTreeMap, sync::Arc};

use serde::de::DeserializeOwned;

use crate::ir::FieldValue;

mod deserializers;

#[cfg(test)]
mod tests;

/// Deserialize Trustfall query results into a Rust struct.
///
/// ```rust
/// # use std::{collections::BTreeMap, sync::Arc};
/// # use maplit::btreemap;
/// # use trustfall_core::ir::FieldValue;
/// #
/// # fn run_query() -> Result<Box<dyn Iterator<Item = BTreeMap<Arc<str>, FieldValue>>>, ()> {
/// #     Ok(Box::new(vec![
/// #        btreemap! {
/// #           Arc::from("number") => FieldValue::Int64(42),
/// #           Arc::from("text") => FieldValue::String("the answer to everything".to_string()),
/// #        }
/// #     ].into_iter()))
/// # }
///
/// use trustfall_core::TryIntoStruct;
///
/// #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
/// struct Output {
///     number: i64,
///     text: String,
/// }
///
/// let results: Vec<_> = run_query()
///     .expect("bad query arguments")
///     .map(|v| v.try_into_struct().expect("struct definition did not match query result shape"))
///     .collect();
///
/// assert_eq!(
///     vec![
///         Output {
///             number: 42,
///             text: "the answer to everything".to_string(),
///         },
///     ],
///     results,
/// );
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
