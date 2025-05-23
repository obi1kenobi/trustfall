use std::{cmp::Ordering, sync::Arc};

/// IR of the values of Trustfall fields.
use async_graphql_value::{ConstValue, Number, Value};
use serde::{Deserialize, Serialize};

/// Values of fields in Trustfall.
///
/// For version that is serialized as an untagged enum, see [TransparentValue].
#[non_exhaustive]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum FieldValue {
    // Order may matter here! Deserialization, if ever configured for untagged serialization,
    // will attempt each variant in order until the first one that matches. Int64 must be
    // above Uint64, which must be above Float64.
    // This is because we want to prioritize the standard Integer GraphQL type over our custom u64,
    // and prioritize exact integers over lossy floats.
    #[default]
    Null,

    /// Together with `Uint64`, corresponds to schemas' `Int` type.
    Int64(i64),

    /// Together with `Int64`, corresponds to schemas' `Int` type.
    Uint64(u64),

    /// Corresponds to schemas' `Float` type. Not allowed to be NaN or infinite.
    Float64(f64),

    String(Arc<str>),
    Boolean(bool),
    Enum(Arc<str>),
    List(Arc<[FieldValue]>),
}

impl FieldValue {
    pub const NULL: Self = FieldValue::Null;
}

impl Default for &FieldValue {
    fn default() -> Self {
        &FieldValue::NULL
    }
}

/// Values of fields in GraphQL types.
///
/// Same as [FieldValue], but serialized as an untagged enum,
/// which may be more suitable e.g. when serializing to JSON.
#[non_exhaustive]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TransparentValue {
    // Order may matter here! Deserialization, if ever configured for untagged serialization,
    // will attempt each variant in order until the first one that matches. Int64 must be
    // above Uint64, which must be above Float64.
    // This is because we want to prioritize the standard Integer GraphQL type over our custom u64,
    // and prioritize exact integers over lossy floats.
    #[default]
    Null,

    /// Together with `Uint64`, corresponds to schemas' `Int` type.
    Int64(i64),

    /// Together with `Int64`, corresponds to schemas' `Int` type.
    Uint64(u64),

    /// Corresponds to schemas' `Float` type. Not allowed to be NaN or infinite.
    Float64(f64),

    String(Arc<str>),
    Boolean(bool),
    Enum(Arc<str>),
    List(Arc<[TransparentValue]>),
}

impl From<FieldValue> for TransparentValue {
    fn from(value: FieldValue) -> Self {
        match value {
            FieldValue::Null => TransparentValue::Null,
            FieldValue::Int64(x) => TransparentValue::Int64(x),
            FieldValue::Uint64(x) => TransparentValue::Uint64(x),
            FieldValue::Float64(x) => TransparentValue::Float64(x),
            FieldValue::String(x) => TransparentValue::String(x),
            FieldValue::Boolean(x) => TransparentValue::Boolean(x),
            FieldValue::Enum(x) => TransparentValue::Enum(x),
            FieldValue::List(x) => TransparentValue::List(
                x.iter().map(|v| v.clone().into()).collect::<Vec<_>>().into(),
            ),
        }
    }
}

impl From<TransparentValue> for FieldValue {
    fn from(value: TransparentValue) -> Self {
        match value {
            TransparentValue::Null => FieldValue::Null,
            TransparentValue::Int64(x) => FieldValue::Int64(x),
            TransparentValue::Uint64(x) => FieldValue::Uint64(x),
            TransparentValue::Float64(x) => FieldValue::Float64(x),
            TransparentValue::String(x) => FieldValue::String(x),
            TransparentValue::Boolean(x) => FieldValue::Boolean(x),
            TransparentValue::Enum(x) => FieldValue::Enum(x),
            TransparentValue::List(x) => {
                FieldValue::List(x.iter().map(|v| v.clone().into()).collect::<Vec<_>>().into())
            }
        }
    }
}

impl FieldValue {
    fn discriminant(&self) -> isize {
        // Ensure this is the same order as the variant order at definition-time.
        match self {
            Self::Null => 0,
            Self::Int64(..) => 1,
            Self::Uint64(..) => 2,
            Self::Float64(..) => 3,
            Self::String(..) => 4,
            Self::Boolean(..) => 5,
            Self::Enum(..) => 6,
            Self::List(..) => 7,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            FieldValue::Uint64(u) => (*u).try_into().ok(),
            FieldValue::Int64(i) => Some(*i),
            FieldValue::Null
            | FieldValue::Float64(_)
            | FieldValue::String(_)
            | FieldValue::Boolean(_)
            | FieldValue::List(_)
            | FieldValue::Enum(_) => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            FieldValue::Uint64(u) => Some(*u),
            FieldValue::Int64(i) => (*i).try_into().ok(),
            FieldValue::Null
            | FieldValue::Float64(_)
            | FieldValue::String(_)
            | FieldValue::Boolean(_)
            | FieldValue::List(_)
            | FieldValue::Enum(_) => None,
        }
    }

    pub fn as_usize(&self) -> Option<usize> {
        match self {
            FieldValue::Uint64(u) => (*u).try_into().ok(),
            FieldValue::Int64(i) => (*i).try_into().ok(),
            FieldValue::Null
            | FieldValue::Float64(_)
            | FieldValue::String(_)
            | FieldValue::Boolean(_)
            | FieldValue::List(_)
            | FieldValue::Enum(_) => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            FieldValue::Float64(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            FieldValue::String(s) => Some(s.as_ref()),
            _ => None,
        }
    }

    pub fn as_arc_str(&self) -> Option<&Arc<str>> {
        match self {
            FieldValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            FieldValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_slice(&self) -> Option<&[FieldValue]> {
        match self {
            FieldValue::List(l) => Some(l.as_ref()),
            _ => None,
        }
    }

    pub fn as_arc_slice(&self) -> Option<&Arc<[FieldValue]>> {
        match self {
            FieldValue::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_vec_with<'a, T>(
        &'a self,
        inner: impl Fn(&'a FieldValue) -> Option<T>,
    ) -> Option<Vec<T>> {
        match self {
            FieldValue::List(l) => {
                let maybe_vec: Option<Vec<T>> = l.iter().map(inner).collect();
                maybe_vec
            }
            _ => None,
        }
    }

    pub(crate) fn structural_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Uint64(l0), Self::Uint64(r0)) => l0 == r0,
            (Self::Int64(l0), Self::Int64(r0)) => l0 == r0,
            (Self::Float64(l0), Self::Float64(r0)) => {
                assert!(l0.is_finite());
                assert!(r0.is_finite());
                l0 == r0
            }
            (Self::String(l0), Self::String(r0)) => l0 == r0,
            (Self::Boolean(l0), Self::Boolean(r0)) => l0 == r0,
            (Self::List(l0), Self::List(r0)) => l0 == r0,
            (Self::Enum(l0), Self::Enum(r0)) => l0 == r0,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }

    fn compare_i64_to_u64(signed: i64, unsigned: u64) -> Ordering {
        if let Ok(conv) = unsigned.try_into() {
            // We succeeded in converting the unsigned value into signed.
            // Compare them on equal terms.
            signed.cmp(&conv)
        } else if let Ok(conv) = u64::try_from(signed) {
            // We succeeded in converting the signed value into unsigned.
            // Compare them on equal terms.
            conv.cmp(&unsigned)
        } else {
            // Both values are out of each other's valid range.
            // Since both values are the same bit width,
            // the signed value must be negative and the unsigned value must be very large.
            Ordering::Less
        }
    }
}

impl PartialOrd for FieldValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if let (Self::Int64(l), Self::Uint64(r)) = (self, other) {
            Some(FieldValue::compare_i64_to_u64(*l, *r))
        } else if let (Self::Uint64(l), Self::Int64(r)) = (self, other) {
            Some(FieldValue::compare_i64_to_u64(*r, *l).reverse())
        } else {
            match (self, other) {
                (Self::Uint64(l0), Self::Uint64(r0)) => l0.partial_cmp(r0),
                (Self::Int64(l0), Self::Int64(r0)) => l0.partial_cmp(r0),
                (Self::Float64(l0), Self::Float64(r0)) => {
                    assert!(l0.is_finite());
                    assert!(r0.is_finite());
                    l0.partial_cmp(r0)
                }
                (Self::String(l0), Self::String(r0)) => l0.partial_cmp(r0),
                (Self::Boolean(l0), Self::Boolean(r0)) => l0.partial_cmp(r0),
                (Self::List(l0), Self::List(r0)) => l0.partial_cmp(r0),
                (Self::Enum(l0), Self::Enum(r0)) => l0.partial_cmp(r0),
                _ => self.discriminant().partial_cmp(&other.discriminant()),
            }
        }
    }
}

impl PartialEq for FieldValue {
    fn eq(&self, other: &Self) -> bool {
        if let (Self::Int64(l), Self::Uint64(r)) = (self, other) {
            FieldValue::compare_i64_to_u64(*l, *r).is_eq()
        } else if let (Self::Uint64(..), Self::Int64(..)) = (self, other) {
            Self::eq(other, self)
        } else {
            self.structural_eq(other)
        }
    }
}

impl Eq for FieldValue {}

impl AsRef<FieldValue> for FieldValue {
    fn as_ref(&self) -> &FieldValue {
        self
    }
}

impl From<Arc<str>> for FieldValue {
    fn from(v: Arc<str>) -> Self {
        Self::String(v)
    }
}

impl From<String> for FieldValue {
    fn from(v: String) -> Self {
        Self::String(v.into())
    }
}

impl From<&String> for FieldValue {
    fn from(v: &String) -> Self {
        Self::String(v.clone().into())
    }
}

impl From<&str> for FieldValue {
    fn from(v: &str) -> Self {
        Self::from(v.to_owned())
    }
}

impl From<bool> for FieldValue {
    fn from(v: bool) -> Self {
        Self::Boolean(v)
    }
}

/// Represents a finite (non-infinite, not-NaN) [f64] value
pub struct FiniteF64(f64);
impl From<FiniteF64> for FieldValue {
    fn from(f: FiniteF64) -> FieldValue {
        FieldValue::Float64(f.0)
    }
}

macro_rules! impl_finite_f64_try_from_float {
    ( $( $Float: ident )+ ) => {
        $(
            impl TryFrom<$Float> for FiniteF64 {
                type Error = ($Float, &'static str);

                fn try_from(v: $Float) -> Result<Self, Self::Error> {
                    if v.is_finite() {
                        Ok(Self(v.into()))
                    } else {
                        Err((v, "not a finite (non-infinite, not-NaN) value"))
                    }
                }
            }
        )+
    }
}

impl_finite_f64_try_from_float!(f32 f64);

macro_rules! impl_field_value_from_int {
    ( $( $Int: ident )+ ) => {
        $(
            impl From<$Int> for FieldValue {
                fn from(v: $Int) -> Self {
                    Self::Int64(v.into())
                }
            }
        )+
    }
}

macro_rules! impl_field_value_from_uint {
    ( $( $Uint: ident )+ ) => {
        $(
            impl From<$Uint> for FieldValue {
                fn from(v: $Uint) -> Self {
                    Self::Uint64(v.into())
                }
            }
        )+
    }
}

impl_field_value_from_int!(i8 i16 i32 i64);
impl_field_value_from_uint!(u8 u16 u32 u64);

impl From<usize> for FieldValue {
    fn from(value: usize) -> Self {
        Self::Uint64(value.try_into().expect("failed to convert usize to u64"))
    }
}

impl From<isize> for FieldValue {
    fn from(value: isize) -> Self {
        Self::Int64(value.try_into().expect("failed to convert isize to i64"))
    }
}

impl TryFrom<Option<f32>> for FieldValue {
    type Error = (f32, &'static str);

    fn try_from(value: Option<f32>) -> Result<Self, Self::Error> {
        match value {
            None => Ok(FieldValue::Null),
            Some(v) => {
                let finite_f64 = FiniteF64::try_from(v);
                finite_f64.map(|x| x.into())
            }
        }
    }
}

impl TryFrom<Option<f64>> for FieldValue {
    type Error = (f64, &'static str);

    fn try_from(value: Option<f64>) -> Result<Self, Self::Error> {
        match value {
            None => Ok(FieldValue::Null),
            Some(v) => Ok(FiniteF64::try_from(v)?.into()),
        }
    }
}

macro_rules! impl_field_value_from_unsigned_nonzero {
    ( $( $NonZero:path )+ ) => {
        $(
            impl From<$NonZero> for FieldValue {
                fn from(v: $NonZero) -> Self {
                    Self::Uint64(v.get().into())
                }
            }
        )+
    }
}

impl_field_value_from_unsigned_nonzero!(
    std::num::NonZeroU8
    std::num::NonZeroU16
    std::num::NonZeroU32
    std::num::NonZeroU64
);

macro_rules! impl_field_value_from_signed_nonzero {
    ( $( $NonZero:path )+ ) => {
        $(
            impl From<$NonZero> for FieldValue {
                fn from(v: $NonZero) -> Self {
                    Self::Int64(v.get().into())
                }
            }
        )+
    }
}

impl_field_value_from_signed_nonzero!(
    std::num::NonZeroI8
    std::num::NonZeroI16
    std::num::NonZeroI32
    std::num::NonZeroI64
);

impl<T: Into<FieldValue>> From<Option<T>> for FieldValue {
    fn from(opt: Option<T>) -> FieldValue {
        match opt {
            Some(inner) => inner.into(),
            None => FieldValue::Null,
        }
    }
}

impl<T: Into<FieldValue>> FromIterator<T> for FieldValue {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        FieldValue::List(iter.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<FieldValue>> From<Vec<T>> for FieldValue {
    fn from(vec: Vec<T>) -> FieldValue {
        vec.into_iter().collect()
    }
}

impl<T: Into<FieldValue> + Clone> From<&[T]> for FieldValue {
    fn from(slice: &[T]) -> FieldValue {
        slice.iter().cloned().collect()
    }
}

/// Converts a JSON number to a [FieldValue]
fn convert_number_to_field_value(n: &Number) -> Result<FieldValue, String> {
    // The order here matters!
    // Int64 must be before Uint64, which must be before Float64.
    // See the comment near the definition of FieldValue for details.
    if let Some(i) = n.as_i64() {
        Ok(FieldValue::Int64(i))
    } else if let Some(u) = n.as_u64() {
        Ok(FieldValue::Uint64(u))
    } else if let Some(f) = n.as_f64() {
        Ok(FieldValue::Float64(f))
    } else {
        unreachable!()
    }
}

impl TryFrom<Value> for FieldValue {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Null => Ok(Self::Null),
            Value::Number(n) => convert_number_to_field_value(&n),
            Value::String(s) => Ok(Self::String(s.into())),
            Value::Boolean(b) => Ok(Self::Boolean(b)),
            Value::List(l) => l.into_iter().map(Self::try_from).collect::<Result<Self, _>>(),
            Value::Enum(n) => {
                // We have an enum value, so we know the variant name but the variant on its own
                // doesn't tell us the name of the enum type it belongs in. We'll have to determine
                // the name of the enum type from context. For now, it's None.
                Ok(Self::Enum(n.to_string().into()))
            }
            Value::Binary(_) => Err(String::from("Binary values are not supported")),
            Value::Variable(_) => Err(String::from("Cannot use a variable reference")),
            Value::Object(_) => Err(String::from("Object values are not supported")),
        }
    }
}

impl TryFrom<ConstValue> for FieldValue {
    type Error = String;

    fn try_from(value: ConstValue) -> Result<Self, Self::Error> {
        value.into_value().try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldValue, FiniteF64};

    #[test]
    fn test_field_value_into() {
        #[track_caller]
        fn test(value: impl Into<FieldValue>, expected: FieldValue) {
            assert_eq!(value.into(), expected);
        }

        test(false, FieldValue::Boolean(false));

        test(123i64, FieldValue::Int64(123));
        test(123u64, FieldValue::Uint64(123));

        test(None::<i64>, FieldValue::Null);
        test(Some::<i64>(123), FieldValue::Int64(123));
        test(Some::<u64>(123), FieldValue::Uint64(123));

        test(FiniteF64::try_from(3.15).unwrap(), FieldValue::Float64(3.15));

        test("a &str", FieldValue::String("a &str".into()));
        test("a String".to_string(), FieldValue::String("a String".into()));

        test(vec![1, 2], FieldValue::List(vec![FieldValue::Int64(1), FieldValue::Int64(2)].into()));

        test(
            ["a String".to_string()].as_slice(),
            FieldValue::List(vec![FieldValue::String("a String".to_string().into())].into()),
        );
    }
}
