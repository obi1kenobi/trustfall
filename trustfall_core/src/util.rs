use std::collections::{btree_map, hash_map, BTreeMap, HashMap};
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    frontend::error::FrontendError,
    graphql_query::{error::ParseError, query::Query},
    interpreter::trace::Trace,
    ir::{FieldValue, IRQuery},
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct DisplayVec<T>(pub Vec<T>);

impl<T: Display> Display for DisplayVec<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "[")?;

        for item in &self.0 {
            writeln!(f, "  {};", item)?;
        }

        write!(f, "]")
    }
}

pub(crate) trait TryCollectUniqueKey<K, V>: Iterator<Item = (K, V)>
where
    K: Ord + Eq + Hash,
{
    fn try_collect_unique(&mut self) -> Result<BTreeMap<K, V>, BTreeMap<K, Vec<V>>> {
        let size_hint = self.size_hint().0;
        let mut map = if size_hint > 0 {
            HashMap::with_capacity(size_hint)
        } else {
            HashMap::new()
        };

        let mut maybe_duplicate: Option<(K, V)> = None;
        for (key, value) in &mut *self {
            // TODO: Update this to avoid the duplicated existence check on the common path
            //       if/when the entry_ref() API is stabilized, as proposed here:
            //       https://github.com/rust-lang/rust/issues/56167#issuecomment-910742027
            #[allow(clippy::map_entry)]
            if map.contains_key(&key) {
                maybe_duplicate = Some((key, value));
                break;
            } else {
                map.insert(key, value);
            }
        }

        if let Some((first_duplicate_key, first_duplicate_value)) = maybe_duplicate {
            let mut duplicate_map: BTreeMap<K, Vec<V>> = BTreeMap::new();

            for (key, value) in map.drain() {
                duplicate_map.entry(key).or_default().push(value);
            }
            duplicate_map
                .get_mut(&first_duplicate_key)
                .unwrap()
                .push(first_duplicate_value);

            for (key, value) in &mut *self {
                duplicate_map.entry(key).or_default().push(value);
            }
            duplicate_map.retain(|_, value| value.len() > 1);

            return Err(duplicate_map);
        }

        Ok(map.into_iter().collect())
    }
}

impl<I, K, V> TryCollectUniqueKey<K, V> for I
where
    I: Iterator<Item = (K, V)>,
    K: Ord + Eq + Hash,
{
}

/// A struct similar to the OccupiedError struct in the not-yet-stable `map_try_insert` feature.
/// For internal use only. The `pub(crate)` visibility ensures that this type never leaks out
/// to the external API of this crate.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct HashMapOccupiedError<'a, K: 'a, V: 'a> {
    /// The entry in the map that was already occupied.
    pub entry: hash_map::OccupiedEntry<'a, K, V>,
    /// The value which was not inserted, because the entry was already occupied.
    pub value: V,
}

pub(crate) trait HashMapTryInsertExt<K, V> {
    // Analogous API to the `try_insert()` function in the not-yet-stable `map_try_insert` feature.
    fn insert_or_error(
        &mut self,
        key: K,
        value: V,
    ) -> Result<&mut V, HashMapOccupiedError<'_, K, V>>;
}

impl<K: Eq + Hash, V> HashMapTryInsertExt<K, V> for HashMap<K, V> {
    fn insert_or_error(
        &mut self,
        key: K,
        value: V,
    ) -> Result<&mut V, HashMapOccupiedError<'_, K, V>> {
        match self.entry(key) {
            hash_map::Entry::Vacant(v) => Ok(v.insert(value)),
            hash_map::Entry::Occupied(entry) => Err(HashMapOccupiedError { entry, value }),
        }
    }
}

/// A struct similar to the OccupiedError struct in the not-yet-stable `map_try_insert` feature.
/// For internal use only. The `pub(crate)` visibility ensures that this type never leaks out
/// to the external API of this crate.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct BTreeMapOccupiedError<'a, K: 'a + Ord, V: 'a> {
    /// The entry in the map that was already occupied.
    pub entry: btree_map::OccupiedEntry<'a, K, V>,
    /// The value which was not inserted, because the entry was already occupied.
    pub value: V,
}

pub(crate) trait BTreeMapTryInsertExt<K: Ord, V> {
    // Analogous API to the `try_insert()` function in the not-yet-stable `map_try_insert` feature.
    fn insert_or_error(
        &mut self,
        key: K,
        value: V,
    ) -> Result<&mut V, BTreeMapOccupiedError<'_, K, V>>;
}

impl<K: Ord, V> BTreeMapTryInsertExt<K, V> for BTreeMap<K, V> {
    fn insert_or_error(
        &mut self,
        key: K,
        value: V,
    ) -> Result<&mut V, BTreeMapOccupiedError<'_, K, V>> {
        match self.entry(key) {
            btree_map::Entry::Vacant(v) => Ok(v.insert(value)),
            btree_map::Entry::Occupied(entry) => Err(BTreeMapOccupiedError { entry, value }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TestGraphQLQuery {
    pub(crate) schema_name: String,

    pub(crate) query: String,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) arguments: BTreeMap<String, FieldValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TestParsedGraphQLQuery {
    pub(crate) schema_name: String,

    pub(crate) query: Query,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) arguments: BTreeMap<String, FieldValue>,
}

#[allow(dead_code)]
pub(crate) type TestParsedGraphQLQueryResult = Result<TestParsedGraphQLQuery, ParseError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TestIRQuery {
    pub(crate) schema_name: String,

    pub(crate) ir_query: IRQuery,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) arguments: BTreeMap<String, FieldValue>,
}

#[allow(dead_code)]
pub(crate) type TestIRQueryResult = Result<TestIRQuery, FrontendError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "DataToken: Serialize, for<'de2> DataToken: Deserialize<'de2>")]
pub(crate) struct TestInterpreterOutputTrace<DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize,
    for<'de2> DataToken: Deserialize<'de2>,
{
    pub(crate) schema_name: String,

    pub(crate) trace: Trace<DataToken>,

    pub(crate) results: Vec<BTreeMap<Arc<str>, FieldValue>>,
}
