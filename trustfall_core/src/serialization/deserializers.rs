use std::{collections::BTreeMap, sync::Arc};

use serde::de::{self, IntoDeserializer};

use crate::ir::FieldValue;

#[derive(Debug, Clone)]
pub(super) struct QueryResultDeserializer {
    query_result: BTreeMap<Arc<str>, FieldValue>,
}

impl QueryResultDeserializer {
    pub(super) fn new(query_result: BTreeMap<Arc<str>, FieldValue>) -> Self {
        Self { query_result }
    }
}

#[derive(Debug, Clone)]
struct QueryResultMapDeserializer<I: Iterator<Item = (Arc<str>, FieldValue)>> {
    iter: I,
    next_value: Option<FieldValue>,
}

impl<I: Iterator<Item = (Arc<str>, FieldValue)>> QueryResultMapDeserializer<I> {
    fn new(iter: I) -> Self {
        Self {
            iter,
            next_value: Default::default(),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("error from deserialize: {0}")]
    Custom(String),
}

impl de::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        Self::Custom(msg.to_string())
    }
}

impl<'de> de::Deserializer<'de> for QueryResultDeserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_map(QueryResultMapDeserializer::new(
            self.query_result.into_iter(),
        ))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

impl<'de, I: Iterator<Item = (Arc<str>, FieldValue)>> de::MapAccess<'de>
    for QueryResultMapDeserializer<I>
{
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        self.iter
            .next()
            .map(|(key, value)| {
                self.next_value = Some(value);
                seed.deserialize(key.into_deserializer())
            })
            .transpose()
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        seed.deserialize(
            self.next_value
                .take()
                .expect("called next_value_seed out of order")
                .into_deserializer(),
        )
    }
}

pub struct FieldValueDeserializer {
    value: FieldValue,
}

impl<'de> de::IntoDeserializer<'de, Error> for FieldValue {
    type Deserializer = FieldValueDeserializer;

    fn into_deserializer(self) -> Self::Deserializer {
        FieldValueDeserializer { value: self }
    }
}

impl<'de> de::Deserializer<'de> for FieldValueDeserializer {
    type Error = Error;

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            FieldValue::Int64(v) => {
                visitor.visit_i8(v.try_into().map_err(<Self::Error as de::Error>::custom)?)
            }
            FieldValue::Uint64(v) => {
                visitor.visit_i8(v.try_into().map_err(<Self::Error as de::Error>::custom)?)
            }
            _ => self.deserialize_any(visitor), // we'll let `deserialize_any()` raise the error
        }
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            FieldValue::Int64(v) => {
                visitor.visit_i16(v.try_into().map_err(<Self::Error as de::Error>::custom)?)
            }
            FieldValue::Uint64(v) => {
                visitor.visit_i16(v.try_into().map_err(<Self::Error as de::Error>::custom)?)
            }
            _ => self.deserialize_any(visitor), // we'll let `deserialize_any()` raise the error
        }
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            FieldValue::Int64(v) => {
                visitor.visit_i32(v.try_into().map_err(<Self::Error as de::Error>::custom)?)
            }
            FieldValue::Uint64(v) => {
                visitor.visit_i32(v.try_into().map_err(<Self::Error as de::Error>::custom)?)
            }
            _ => self.deserialize_any(visitor), // we'll let `deserialize_any()` raise the error
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            FieldValue::Int64(v) => {
                visitor.visit_u8(v.try_into().map_err(<Self::Error as de::Error>::custom)?)
            }
            FieldValue::Uint64(v) => {
                visitor.visit_u8(v.try_into().map_err(<Self::Error as de::Error>::custom)?)
            }
            _ => self.deserialize_any(visitor), // we'll let `deserialize_any()` raise the error
        }
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            FieldValue::Int64(v) => {
                visitor.visit_u16(v.try_into().map_err(<Self::Error as de::Error>::custom)?)
            }
            FieldValue::Uint64(v) => {
                visitor.visit_u16(v.try_into().map_err(<Self::Error as de::Error>::custom)?)
            }
            _ => self.deserialize_any(visitor), // we'll let `deserialize_any()` raise the error
        }
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            FieldValue::Int64(v) => {
                visitor.visit_u32(v.try_into().map_err(<Self::Error as de::Error>::custom)?)
            }
            FieldValue::Uint64(v) => {
                visitor.visit_u32(v.try_into().map_err(<Self::Error as de::Error>::custom)?)
            }
            _ => self.deserialize_any(visitor), // we'll let `deserialize_any()` raise the error
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            FieldValue::Float64(v) => visitor.visit_f32(v as f32),
            _ => self.deserialize_any(visitor), // we'll let `deserialize_any()` raise the error
        }
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        if let FieldValue::List(v) = &self.value {
            if len != v.len() {
                return Err(Self::Error::Custom(format!(
                    "cannot deserialize {} length list into {len} sized tuple",
                    v.len()
                )));
            }
        }
        self.deserialize_any(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        match &self.value {
            &FieldValue::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_none()
    }

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        match self.value {
            FieldValue::Null => visitor.visit_none(),
            FieldValue::Int64(v) => visitor.visit_i64(v),
            FieldValue::Uint64(v) => visitor.visit_u64(v),
            FieldValue::Float64(v) => visitor.visit_f64(v),
            FieldValue::String(v) => visitor.visit_string(v),
            FieldValue::Boolean(v) => visitor.visit_bool(v),
            FieldValue::DateTimeUtc(_) => todo!(),
            FieldValue::Enum(_) => todo!(),
            FieldValue::List(v) => visitor.visit_seq(v.into_deserializer()),
        }
    }

    serde::forward_to_deserialize_any! {
        bool i64 i128 u64 u128 f64 char str string seq
        bytes byte_buf unit unit_struct newtype_struct
        tuple_struct map enum struct identifier
    }
}
