use std::{collections::BTreeMap, fmt, sync::Arc};

use async_graphql_parser::types::Type;
use serde::{self, de::Visitor, Deserializer, Serialize, Serializer};

pub fn serde_type_serializer<S>(value: &Type, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    value.to_string().serialize(serializer)
}

pub fn serde_type_deserializer<'de, D>(deserializer: D) -> Result<Type, D::Error>
where
    D: Deserializer<'de>,
{
    struct TypeDeserializer;

    impl<'de> Visitor<'de> for TypeDeserializer {
        type Value = Type;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("GraphQL type")
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let ty =
                Type::new(s).ok_or_else(|| serde::de::Error::custom("not a valid GraphQL type"))?;
            Ok(ty)
        }
    }

    deserializer.deserialize_str(TypeDeserializer)
}

pub fn serde_variables_serializer<S>(
    value: &BTreeMap<Arc<str>, Type>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let converted: BTreeMap<&str, String> = value
        .iter()
        .map(|(k, v)| (k.as_ref(), v.to_string()))
        .collect();
    converted.serialize(serializer)
}

pub fn serde_variables_deserializer<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<Arc<str>, Type>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TypeDeserializer;

    impl<'de> Visitor<'de> for TypeDeserializer {
        type Value = BTreeMap<Arc<str>, &'de str>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("map of variable names -> types")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut result: BTreeMap<Arc<str>, &'de str> = BTreeMap::new();
            while let Some((key, value)) = map.next_entry()? {
                result.insert(key, value);
            }
            Ok(result)
        }
    }

    deserializer.deserialize_map(TypeDeserializer).map(|value| {
        let mut result: BTreeMap<Arc<str>, Type> = Default::default();
        for (k, v) in value {
            let ty = Type::new(v).unwrap();
            result.insert(k, ty);
        }
        result
    })
}
