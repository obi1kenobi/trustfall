#![allow(dead_code)]

pub mod indexed;

use std::{collections::BTreeMap, convert::TryFrom, fmt, fmt::Debug, num::NonZeroUsize, sync::Arc};

use async_graphql_parser::types::{BaseType, Type};
use async_graphql_value::{ConstValue, Number, Value};
use chrono::{DateTime, Utc};
use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

use crate::frontend::error::FilterTypeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Vid(pub(crate) NonZeroUsize); // vertex ID

impl Vid {
    pub fn new(id: NonZeroUsize) -> Vid {
        Vid(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Eid(pub(crate) NonZeroUsize); // edge ID

impl Eid {
    pub fn new(id: NonZeroUsize) -> Eid {
        Eid(id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldValue {
    // Order may matter here! Deserialization, if ever configured for untagged serialization,
    // will attempt each variant in order until the first one that matches. Int64 must be
    // above Uint64, which must be above Float64.
    // This is because we want to prioritize the standard Integer GraphQL type over our custom u64,
    // and prioritize exact integers over lossy floats.
    Null,
    Int64(i64), // AKA Integer
    Uint64(u64),
    Float64(f64), // AKA Float, and also not allowed to be NaN
    String(String),
    Boolean(bool),
    DateTimeUtc(DateTime<Utc>),
    Enum(String),
    List(Vec<FieldValue>),
}

/// Same as FieldValue, but serialized as an untagged enum,
/// which may be more suitable e.g. when serializing to JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TransparentValue {
    // Order may matter here! Deserialization, if ever configured for untagged serialization,
    // will attempt each variant in order until the first one that matches. Int64 must be
    // above Uint64, which must be above Float64.
    // This is because we want to prioritize the standard Integer GraphQL type over our custom u64,
    // and prioritize exact integers over lossy floats.
    Null,
    Int64(i64), // AKA Integer
    Uint64(u64),
    Float64(f64), // AKA Float, and also not allowed to be NaN
    String(String),
    Boolean(bool),
    DateTimeUtc(DateTime<Utc>),
    Enum(String),
    List(Vec<FieldValue>),
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
            FieldValue::DateTimeUtc(x) => TransparentValue::DateTimeUtc(x),
            FieldValue::Enum(x) => TransparentValue::Enum(x),
            FieldValue::List(x) => TransparentValue::List(x),
        }
    }
}

impl FieldValue {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            FieldValue::Uint64(u) => (*u).try_into().ok(),
            FieldValue::Int64(i) => Some(*i),
            FieldValue::Null
            | FieldValue::Float64(_)
            | FieldValue::String(_)
            | FieldValue::Boolean(_)
            | FieldValue::DateTimeUtc(_)
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
            | FieldValue::DateTimeUtc(_)
            | FieldValue::List(_)
            | FieldValue::Enum(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            FieldValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

impl PartialEq for FieldValue {
    fn eq(&self, other: &Self) -> bool {
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
            (Self::DateTimeUtc(l0), Self::DateTimeUtc(r0)) => l0 == r0,
            (Self::List(l0), Self::List(r0)) => l0 == r0,
            (Self::Enum(l0), Self::Enum(r0)) => l0 == r0,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl Eq for FieldValue {}

impl AsRef<FieldValue> for FieldValue {
    fn as_ref(&self) -> &FieldValue {
        self
    }
}

impl From<String> for FieldValue {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

impl From<&String> for FieldValue {
    fn from(v: &String) -> Self {
        Self::String(v.clone())
    }
}

impl From<&str> for FieldValue {
    fn from(v: &str) -> Self {
        Self::String(v.to_string())
    }
}

impl From<bool> for FieldValue {
    fn from(v: bool) -> Self {
        Self::Boolean(v)
    }
}

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

            impl From<&$Int> for FieldValue {
                fn from(v: &$Int) -> Self {
                    Self::Int64((*v).into())
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

            impl From<&$Uint> for FieldValue {
                fn from(v: &$Uint) -> Self {
                    Self::Uint64((*v).into())
                }
            }
        )+
    }
}

impl_field_value_from_int!(i8 i16 i32 i64);
impl_field_value_from_uint!(u8 u16 u32 u64);

impl From<DateTime<Utc>> for FieldValue {
    fn from(v: DateTime<Utc>) -> Self {
        Self::DateTimeUtc(v)
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
            Some(v) => {
                let finite_f64 = FiniteF64::try_from(v);
                finite_f64.map(|x| x.into())
            }
        }
    }
}

impl<T: Clone + Into<FieldValue>> From<&Option<T>> for FieldValue {
    fn from(opt: &Option<T>) -> FieldValue {
        match opt {
            Some(inner) => inner.clone().into(),
            None => FieldValue::Null,
        }
    }
}

impl<T: Into<FieldValue>> From<Option<T>> for FieldValue {
    fn from(opt: Option<T>) -> FieldValue {
        match opt {
            Some(inner) => inner.into(),
            None => FieldValue::Null,
        }
    }
}

impl<T: Into<FieldValue>> From<Vec<T>> for FieldValue {
    fn from(mut v: Vec<T>) -> FieldValue {
        FieldValue::List(v.drain(..).map(|x| x.into()).collect())
    }
}

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

impl TryFrom<&Value> for FieldValue {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::Null => Ok(Self::Null),
            Value::Number(n) => convert_number_to_field_value(n),
            Value::String(s) => Ok(Self::String(s.to_owned())),
            Value::Boolean(b) => Ok(Self::Boolean(*b)),
            Value::List(l) => Ok(Self::List(l.iter().try_fold(
                vec![],
                |mut acc, v| -> Result<Vec<FieldValue>, Self::Error> {
                    acc.push(Self::try_from(v)?);
                    Ok(acc)
                },
            )?)),
            Value::Enum(n) => {
                // We have an enum value, so we know the variant name but the variant on its own
                // doesn't tell us the name of the enum type it belongs in. We'll have to determine
                // the name of the enum type from context. For now, it's None.
                Ok(Self::Enum(n.to_string()))
            }
            Value::Binary(_) => Err(String::from("Binary values are not supported")),
            Value::Variable(_) => Err(String::from("Cannot use a variable reference")),
            Value::Object(_) => Err(String::from("Object values are not supported")),
        }
    }
}

impl TryFrom<Value> for FieldValue {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Number(n) => convert_number_to_field_value(&n),
            Value::String(s) => Ok(Self::String(s)),
            _ => Self::try_from(&value),
        }
    }
}

impl TryFrom<&ConstValue> for FieldValue {
    type Error = String;

    fn try_from(value: &ConstValue) -> Result<Self, Self::Error> {
        FieldValue::try_from(value.clone().into_value())
    }
}

impl TryFrom<ConstValue> for FieldValue {
    type Error = String;

    fn try_from(value: ConstValue) -> Result<Self, Self::Error> {
        FieldValue::try_from(value.into_value())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeParameters(
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")] pub BTreeMap<Arc<str>, FieldValue>,
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRQueryComponent {
    pub root: Vid,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vertices: BTreeMap<Vid, IRVertex>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub edges: BTreeMap<Eid, Arc<IREdge>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub folds: BTreeMap<Eid, Arc<IRFold>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: BTreeMap<Arc<str>, ContextField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRQuery {
    pub root_name: Arc<str>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_parameters: Option<Arc<EdgeParameters>>,

    pub root_component: Arc<IRQueryComponent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IREdge {
    pub eid: Eid,
    pub from_vid: Vid,
    pub to_vid: Vid,
    pub edge_name: Arc<str>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Arc<EdgeParameters>>,

    #[serde(default = "default_optional", skip_serializing_if = "is_false")]
    pub optional: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recursive: Option<NonZeroUsize>,
}

fn default_optional() -> bool {
    false
}

fn is_false(b: &bool) -> bool {
    !b
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRVertex {
    pub vid: Vid,
    pub type_name: Arc<str>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coerced_from_type: Option<Arc<str>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Operation<LocalField, Argument>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRFold {
    pub eid: Eid,
    pub from_vid: Vid,
    pub to_vid: Vid,
    pub edge_name: Arc<str>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Arc<EdgeParameters>>,

    pub component: Arc<IRQueryComponent>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub post_filters: Arc<Vec<Operation<LocalField, Argument>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Argument {
    Tag(ContextField),
    Variable(VariableRef),
}

impl Argument {
    pub(crate) fn as_tag(&self) -> Option<&ContextField> {
        match self {
            Argument::Tag(t) => Some(t),
            Argument::Variable(_) => None,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation<LeftT, RightT>
where
    LeftT: Debug + Clone + PartialEq + Eq,
    RightT: Debug + Clone + PartialEq + Eq,
{
    IsNull(LeftT),
    IsNotNull(LeftT),
    Equals(LeftT, RightT),
    NotEquals(LeftT, RightT),
    LessThan(LeftT, RightT),
    LessThanOrEqual(LeftT, RightT),
    GreaterThan(LeftT, RightT),
    GreaterThanOrEqual(LeftT, RightT),
    Contains(LeftT, RightT),
    NotContains(LeftT, RightT),
    OneOf(LeftT, RightT),
    NotOneOf(LeftT, RightT),
    HasPrefix(LeftT, RightT),
    NotHasPrefix(LeftT, RightT),
    HasSuffix(LeftT, RightT),
    NotHasSuffix(LeftT, RightT),
    HasSubstring(LeftT, RightT),
    NotHasSubstring(LeftT, RightT),
    RegexMatches(LeftT, RightT),
    NotRegexMatches(LeftT, RightT),
}

impl<LeftT, RightT> Operation<LeftT, RightT>
where
    LeftT: Debug + Clone + PartialEq + Eq,
    RightT: Debug + Clone + PartialEq + Eq,
{
    pub(crate) fn left(&self) -> &LeftT {
        match self {
            Operation::IsNull(left) => left,
            Operation::IsNotNull(left) => left,
            Operation::Equals(left, _) => left,
            Operation::NotEquals(left, _) => left,
            Operation::LessThan(left, _) => left,
            Operation::LessThanOrEqual(left, _) => left,
            Operation::GreaterThan(left, _) => left,
            Operation::GreaterThanOrEqual(left, _) => left,
            Operation::Contains(left, _) => left,
            Operation::NotContains(left, _) => left,
            Operation::OneOf(left, _) => left,
            Operation::NotOneOf(left, _) => left,
            Operation::HasPrefix(left, _) => left,
            Operation::NotHasPrefix(left, _) => left,
            Operation::HasSuffix(left, _) => left,
            Operation::NotHasSuffix(left, _) => left,
            Operation::HasSubstring(left, _) => left,
            Operation::NotHasSubstring(left, _) => left,
            Operation::RegexMatches(left, _) => left,
            Operation::NotRegexMatches(left, _) => left,
        }
    }

    pub(crate) fn right(&self) -> Option<&RightT> {
        match self {
            Operation::IsNull(_) | Operation::IsNotNull(_) => None,
            Operation::Equals(_, right) => Some(right),
            Operation::NotEquals(_, right) => Some(right),
            Operation::LessThan(_, right) => Some(right),
            Operation::LessThanOrEqual(_, right) => Some(right),
            Operation::GreaterThan(_, right) => Some(right),
            Operation::GreaterThanOrEqual(_, right) => Some(right),
            Operation::Contains(_, right) => Some(right),
            Operation::NotContains(_, right) => Some(right),
            Operation::OneOf(_, right) => Some(right),
            Operation::NotOneOf(_, right) => Some(right),
            Operation::HasPrefix(_, right) => Some(right),
            Operation::NotHasPrefix(_, right) => Some(right),
            Operation::HasSuffix(_, right) => Some(right),
            Operation::NotHasSuffix(_, right) => Some(right),
            Operation::HasSubstring(_, right) => Some(right),
            Operation::NotHasSubstring(_, right) => Some(right),
            Operation::RegexMatches(_, right) => Some(right),
            Operation::NotRegexMatches(_, right) => Some(right),
        }
    }

    pub(crate) fn operation_name(&self) -> &'static str {
        match self {
            Operation::IsNull(..) => "is_null",
            Operation::IsNotNull(..) => "is_not_null",
            Operation::Equals(..) => "=",
            Operation::NotEquals(..) => "!=",
            Operation::LessThan(..) => "<",
            Operation::LessThanOrEqual(..) => "<=",
            Operation::GreaterThan(..) => ">",
            Operation::GreaterThanOrEqual(..) => ">=",
            Operation::Contains(..) => "contains",
            Operation::NotContains(..) => "not_contains",
            Operation::OneOf(..) => "one_of",
            Operation::NotOneOf(..) => "not_one_of",
            Operation::HasPrefix(..) => "has_prefix",
            Operation::NotHasPrefix(..) => "not_has_prefix",
            Operation::HasSuffix(..) => "has_suffix",
            Operation::NotHasSuffix(..) => "not_has_suffix",
            Operation::HasSubstring(..) => "has_substring",
            Operation::NotHasSubstring(..) => "not_has_substring",
            Operation::RegexMatches(..) => "regex",
            Operation::NotRegexMatches(..) => "not_regex",
        }
    }

    pub(crate) fn try_map<LeftF, LeftOutT, RightF, RightOutT, Err>(
        &self,
        map_left: LeftF,
        map_right: RightF,
    ) -> Result<Operation<LeftOutT, RightOutT>, Err>
    where
        LeftOutT: Debug + Clone + PartialEq + Eq,
        RightOutT: Debug + Clone + PartialEq + Eq,
        LeftF: FnOnce(&LeftT) -> Result<LeftOutT, Err>,
        RightF: FnOnce(&RightT) -> Result<RightOutT, Err>,
    {
        Ok(match self {
            Operation::IsNull(left) => Operation::IsNull(map_left(left)?),
            Operation::IsNotNull(left) => Operation::IsNotNull(map_left(left)?),
            Operation::Equals(left, right) => Operation::Equals(map_left(left)?, map_right(right)?),
            Operation::NotEquals(left, right) => {
                Operation::NotEquals(map_left(left)?, map_right(right)?)
            }
            Operation::LessThan(left, right) => {
                Operation::LessThan(map_left(left)?, map_right(right)?)
            }
            Operation::LessThanOrEqual(left, right) => {
                Operation::LessThanOrEqual(map_left(left)?, map_right(right)?)
            }
            Operation::GreaterThan(left, right) => {
                Operation::GreaterThan(map_left(left)?, map_right(right)?)
            }
            Operation::GreaterThanOrEqual(left, right) => {
                Operation::GreaterThanOrEqual(map_left(left)?, map_right(right)?)
            }
            Operation::Contains(left, right) => {
                Operation::Contains(map_left(left)?, map_right(right)?)
            }
            Operation::NotContains(left, right) => {
                Operation::NotContains(map_left(left)?, map_right(right)?)
            }
            Operation::OneOf(left, right) => Operation::OneOf(map_left(left)?, map_right(right)?),
            Operation::NotOneOf(left, right) => {
                Operation::NotOneOf(map_left(left)?, map_right(right)?)
            }
            Operation::HasPrefix(left, right) => {
                Operation::HasPrefix(map_left(left)?, map_right(right)?)
            }
            Operation::NotHasPrefix(left, right) => {
                Operation::NotHasPrefix(map_left(left)?, map_right(right)?)
            }
            Operation::HasSuffix(left, right) => {
                Operation::HasSuffix(map_left(left)?, map_right(right)?)
            }
            Operation::NotHasSuffix(left, right) => {
                Operation::NotHasSuffix(map_left(left)?, map_right(right)?)
            }
            Operation::HasSubstring(left, right) => {
                Operation::HasSubstring(map_left(left)?, map_right(right)?)
            }
            Operation::NotHasSubstring(left, right) => {
                Operation::NotHasSubstring(map_left(left)?, map_right(right)?)
            }
            Operation::RegexMatches(left, right) => {
                Operation::RegexMatches(map_left(left)?, map_right(right)?)
            }
            Operation::NotRegexMatches(left, right) => {
                Operation::NotRegexMatches(map_left(left)?, map_right(right)?)
            }
        })
    }
}

impl Operation<LocalField, Argument> {
    pub(crate) fn operand_types_valid(
        &self,
        tag_name: Option<&str>,
    ) -> Result<(), FilterTypeError> {
        let left = self.left();
        let right = self.right();
        let left_type = &left.field_type;
        let right_type = right.map(|arg| match arg {
            Argument::Tag(tag) => &tag.field_type,
            Argument::Variable(var) => &var.variable_type,
        });

        // Check the left and right operands match the operator's needs individually.
        // For example:
        // - Check that nullability filters aren't applied to fields that are already non-nullable.
        // - Check that string-like filters aren't used with non-string operands.
        //
        // Also check that the left and right operands have the appropriate relationship with
        // each other when considering the operand they are used with. For example:
        // - Check that filtering with "=" happens between equal types, ignoring nullability.
        // - Check that filtering with "contains" happens with a left-hand type that is a
        //   (maybe non-nullable) list of a maybe-nullable version of the right-hand type.
        match self {
            Operation::IsNull(_) | Operation::IsNotNull(_) => {
                // Checking non-nullable types for null or non-null is pointless.
                if left_type.nullable {
                    Ok(())
                } else {
                    Err(FilterTypeError::NonNullableTypeFilteredForNullability(
                        self.operation_name().to_owned(),
                        left.field_name.to_string(),
                        left.field_type.to_string(),
                        matches!(self, Operation::IsNotNull(..)),
                    ))
                }
            }
            Operation::Equals(_, _) | Operation::NotEquals(_, _) => {
                // Individually, any operands are valid for equality operations.
                //
                // For the operands relative to each other, nullability doesn't matter,
                // but the rest of the type must be the same.
                let right_type = right_type.unwrap();
                if are_base_types_equal_ignoring_nullability(&left_type.base, &right_type.base) {
                    Ok(())
                } else {
                    // The right argument must be a tag at this point. If it is not a tag
                    // and the second .unwrap() below panics, then our type inference
                    // has inferred an incorrect type for the variable in the argument.
                    let tag = right.unwrap().as_tag().unwrap();

                    Err(FilterTypeError::TypeMismatchBetweenTagAndFilter(
                        self.operation_name().to_string(),
                        left.field_name.to_string(),
                        left_type.to_string(),
                        tag_name.unwrap().to_string(),
                        tag.field_name.to_string(),
                        tag.field_type.to_string(),
                    ))
                }
            }
            Operation::LessThan(_, _)
            | Operation::LessThanOrEqual(_, _)
            | Operation::GreaterThan(_, _)
            | Operation::GreaterThanOrEqual(_, _) => {
                // Individually, the operands' types must be non-nullable or list, recursively,
                // versions of orderable types.
                let right_type = right_type.unwrap();

                let mut errors = vec![];
                if !is_base_type_orderable(&left_type.base) {
                    errors.push(FilterTypeError::OrderingFilterOperationOnNonOrderableField(
                        self.operation_name().to_string(),
                        left.field_name.to_string(),
                        left_type.to_string(),
                    ));
                }

                if !is_base_type_orderable(&right_type.base) {
                    // The right argument must be a tag at this point. If it is not a tag
                    // and the second .unwrap() below panics, then our type inference
                    // has inferred an incorrect type for the variable in the argument.
                    let tag = right.unwrap().as_tag().unwrap();

                    errors.push(FilterTypeError::OrderingFilterOperationOnNonOrderableTag(
                        self.operation_name().to_string(),
                        tag_name.unwrap().to_string(),
                        tag.field_name.to_string(),
                        tag.field_type.to_string(),
                    ));
                }

                // For the operands relative to each other, nullability doesn't matter,
                // but the types must be equal to each other.
                if !are_base_types_equal_ignoring_nullability(&left_type.base, &right_type.base) {
                    // The right argument must be a tag at this point. If it is not a tag
                    // and the second .unwrap() below panics, then our type inference
                    // has inferred an incorrect type for the variable in the argument.
                    let tag = right.unwrap().as_tag().unwrap();

                    errors.push(FilterTypeError::TypeMismatchBetweenTagAndFilter(
                        self.operation_name().to_string(),
                        left.field_name.to_string(),
                        left_type.to_string(),
                        tag_name.unwrap().to_string(),
                        tag.field_name.to_string(),
                        tag.field_type.to_string(),
                    ));
                }

                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(errors.into())
                }
            }
            Operation::Contains(_, _) | Operation::NotContains(_, _) => {
                // The left-hand operand needs to be a list, ignoring nullability.
                // The right-hand operand may be anything, if considered individually.
                let inner_type = match &left_type.base {
                    BaseType::List(ty) => Ok(ty),
                    BaseType::Named(_) => Err(FilterTypeError::ListFilterOperationOnNonListField(
                        self.operation_name().to_string(),
                        left.field_name.to_string(),
                        left.field_type.to_string(),
                    )),
                }?;

                let right_type = right_type.unwrap();

                // However, the type inside the left-hand list must be equal,
                // ignoring nullability, to the type of the right-hand operand.
                if are_base_types_equal_ignoring_nullability(&inner_type.base, &right_type.base) {
                    Ok(())
                } else {
                    // The right argument must be a tag at this point. If it is not a tag
                    // and the second .unwrap() below panics, then our type inference
                    // has inferred an incorrect type for the variable in the argument.
                    let tag = right.unwrap().as_tag().unwrap();

                    Err(FilterTypeError::TypeMismatchBetweenTagAndFilter(
                        self.operation_name().to_string(),
                        left.field_name.to_string(),
                        left_type.to_string(),
                        tag_name.unwrap().to_string(),
                        tag.field_name.to_string(),
                        tag.field_type.to_string(),
                    ))
                }
            }
            Operation::OneOf(_, _) | Operation::NotOneOf(_, _) => {
                // The right-hand operand needs to be a list, ignoring nullability.
                // The left-hand operand may be anything, if considered individually.
                let right_type = right_type.unwrap();
                let inner_type = match &right_type.base {
                    BaseType::List(ty) => Ok(ty),
                    BaseType::Named(_) => {
                        // The right argument must be a tag at this point. If it is not a tag
                        // and the second .unwrap() below panics, then our type inference
                        // has inferred an incorrect type for the variable in the argument.
                        let tag = right.unwrap().as_tag().unwrap();

                        Err(FilterTypeError::ListFilterOperationOnNonListTag(
                            self.operation_name().to_string(),
                            tag_name.unwrap().to_string(),
                            tag.field_name.to_string(),
                            tag.field_type.to_string(),
                        ))
                    }
                }?;

                // However, the type inside the right-hand list must be equal,
                // ignoring nullability, to the type of the left-hand operand.
                if are_base_types_equal_ignoring_nullability(&left_type.base, &inner_type.base) {
                    Ok(())
                } else {
                    // The right argument must be a tag at this point. If it is not a tag
                    // and the second .unwrap() below panics, then our type inference
                    // has inferred an incorrect type for the variable in the argument.
                    let tag = right.unwrap().as_tag().unwrap();

                    Err(FilterTypeError::TypeMismatchBetweenTagAndFilter(
                        self.operation_name().to_string(),
                        left.field_name.to_string(),
                        left_type.to_string(),
                        tag_name.unwrap().to_string(),
                        tag.field_name.to_string(),
                        tag.field_type.to_string(),
                    ))
                }
            }
            Operation::HasPrefix(_, _)
            | Operation::NotHasPrefix(_, _)
            | Operation::HasSuffix(_, _)
            | Operation::NotHasSuffix(_, _)
            | Operation::HasSubstring(_, _)
            | Operation::NotHasSubstring(_, _)
            | Operation::RegexMatches(_, _)
            | Operation::NotRegexMatches(_, _) => {
                let mut errors = vec![];

                // Both operands need to be strings, ignoring nullability.
                match &left_type.base {
                    BaseType::Named(ty) if ty == "String" => {}
                    _ => {
                        errors.push(FilterTypeError::StringFilterOperationOnNonStringField(
                            self.operation_name().to_string(),
                            left.field_name.to_string(),
                            left_type.to_string(),
                        ));
                    }
                };

                match &right_type.unwrap().base {
                    BaseType::Named(ty) if ty == "String" => {}
                    _ => {
                        // The right argument must be a tag at this point. If it is not a tag
                        // and the second .unwrap() below panics, then our type inference
                        // has inferred an incorrect type for the variable in the argument.
                        let tag = right.unwrap().as_tag().unwrap();
                        errors.push(FilterTypeError::StringFilterOperationOnNonStringTag(
                            self.operation_name().to_string(),
                            tag_name.unwrap().to_string(),
                            tag.field_name.to_string(),
                            tag.field_type.to_string(),
                        ));
                    }
                }

                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(errors.into())
                }
            }
        }
    }
}

fn are_base_types_equal_ignoring_nullability(left: &BaseType, right: &BaseType) -> bool {
    match (left, right) {
        (BaseType::Named(l), BaseType::Named(r)) => l == r,
        (BaseType::List(l), BaseType::List(r)) => {
            are_base_types_equal_ignoring_nullability(&l.base, &r.base)
        }
        (BaseType::Named(_), BaseType::List(_)) | (BaseType::List(_), BaseType::Named(_)) => false,
    }
}

fn is_base_type_orderable(operand_type: &BaseType) -> bool {
    match operand_type {
        BaseType::Named(name) => {
            name == "Int" || name == "Float" || name == "String" || name == "DateTime"
        }
        BaseType::List(l) => is_base_type_orderable(&l.base),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextField {
    pub vertex_id: Vid,

    pub field_name: Arc<str>,

    #[serde(serialize_with = "serde_type_serializer")]
    #[serde(deserialize_with = "serde_type_deserializer")]
    pub field_type: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalField {
    pub field_name: Arc<str>,

    #[serde(serialize_with = "serde_type_serializer")]
    #[serde(deserialize_with = "serde_type_deserializer")]
    pub field_type: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariableRef {
    pub variable_name: Arc<str>,

    #[serde(serialize_with = "serde_type_serializer")]
    #[serde(deserialize_with = "serde_type_deserializer")]
    pub variable_type: Type,
}

pub(crate) fn serde_type_serializer<S>(value: &Type, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    value.to_string().serialize(serializer)
}

pub(crate) fn serde_type_deserializer<'de, D>(deserializer: D) -> Result<Type, D::Error>
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

#[cfg(test)]
mod tests {
    use super::FieldValue;

    fn serialize_then_deserialize(value: &FieldValue) -> FieldValue {
        ron::from_str(ron::to_string(value).unwrap().as_str()).unwrap()
    }

    #[test]
    fn serialize_then_deserialize_enum() {
        let value = FieldValue::Enum("foo".to_string());
        let deserialized: FieldValue = serialize_then_deserialize(&value);
        assert_eq!(
            value,
            deserialized,
            "Serialized as: {}",
            ron::to_string(&value).unwrap()
        );
    }

    #[test]
    fn serialize_then_deserialize_list() {
        let value = FieldValue::List(vec![
            FieldValue::Int64(1),
            FieldValue::Int64(2),
            FieldValue::String("foo".into()),
        ]);
        let deserialized: FieldValue = serialize_then_deserialize(&value);
        assert_eq!(
            value,
            deserialized,
            "Serialized as: {}",
            ron::to_string(&value).unwrap()
        );
    }

    #[test]
    fn serialize_then_deserialize_float() {
        let value = FieldValue::Float64(1.0);
        let deserialized: FieldValue = serialize_then_deserialize(&value);
        assert_eq!(
            value,
            deserialized,
            "Serialized as: {}",
            ron::to_string(&value).unwrap()
        );
    }

    #[test]
    fn serialize_then_deserialize_i64() {
        let value = FieldValue::Int64(-123);
        let deserialized: FieldValue = serialize_then_deserialize(&value);
        assert_eq!(
            value,
            deserialized,
            "Serialized as: {}",
            ron::to_string(&value).unwrap()
        );
    }

    #[test]
    fn serialize_then_deserialize_u64() {
        let value = FieldValue::Uint64((i64::MAX as u64) + 1);
        let deserialized: FieldValue = serialize_then_deserialize(&value);
        assert_eq!(
            value,
            deserialized,
            "Serialized as: {}",
            ron::to_string(&value).unwrap()
        );
    }
}
