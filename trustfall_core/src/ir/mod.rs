//! Trustfall intermediate representation (IR)

mod indexed;
pub mod types;
pub mod value;

use std::{
    cmp::Ordering, collections::BTreeMap, fmt::Debug, num::NonZeroUsize, ops::Index, sync::Arc,
};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::frontend::error::FilterTypeError;

pub use self::indexed::{EdgeKind, IndexedQuery, InvalidIRQueryError, Output};
use self::types::is_base_type_orderable;
pub use self::types::{NamedTypedValue, Type};
pub use self::value::{FieldValue, TransparentValue};

pub(crate) const TYPENAME_META_FIELD: &str = "__typename";

pub(crate) static TYPENAME_META_FIELD_ARC: Lazy<Arc<str>> =
    Lazy::new(|| Arc::from(TYPENAME_META_FIELD));

/// Unique vertex ID identifying a specific vertex in a Trustfall query
#[doc(alias("vertex", "node"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Vid(pub(crate) NonZeroUsize);

impl Vid {
    pub fn new(id: NonZeroUsize) -> Vid {
        Vid(id)
    }
}

/// Unique edge ID identifying a specific edge in a Trustfall query
#[doc(alias = "edge")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Eid(pub(crate) NonZeroUsize);

impl Eid {
    pub fn new(id: NonZeroUsize) -> Eid {
        Eid(id)
    }
}

/// Parameter values for an edge expansion.
///
/// Passed as an argument to the [`Adapter::resolve_starting_vertices`] and
/// [`Adapter::resolve_neighbors`] functions. In those cases, the caller guarantees that
/// all edge parameters marked as required in the schema are included in
/// the [`EdgeParameters`] value.
///
/// [`Adapter::resolve_starting_vertices`]: crate::interpreter::Adapter::resolve_neighbors
/// [`Adapter::resolve_neighbors`]: crate::interpreter::Adapter::resolve_neighbors
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeParameters {
    pub(crate) contents: Arc<BTreeMap<Arc<str>, FieldValue>>,
}

impl EdgeParameters {
    pub(crate) fn new(contents: Arc<BTreeMap<Arc<str>, FieldValue>>) -> Self {
        Self { contents }
    }

    /// Gets the value of the edge parameter by this name.
    ///
    /// Returns `None` if the edge parameter was not present.
    /// Returns the default value if the query did not set a value but the edge parameter
    /// defined a default value.
    pub fn get(&self, name: &str) -> Option<&FieldValue> {
        self.contents.get(name)
    }

    /// Iterates through all the edge parameters and their values.
    pub fn iter(&self) -> impl Iterator<Item = (&'_ Arc<str>, &'_ FieldValue)> + '_ {
        self.contents.iter()
    }

    /// Returns `true` if the edge has any parameters, and `false` otherwise.
    pub fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }
}

/// Enable indexing into [`EdgeParameters`] values: `parameters["param_name"]`
impl<'a> Index<&'a str> for EdgeParameters {
    type Output = FieldValue;

    fn index(&self, index: &'a str) -> &Self::Output {
        &self.contents[index]
    }
}

/// A complete component of a query; may itself contain one or more components.
///
/// Contains information about the Vid where the component is rooted,
/// as well as well as maps of all vertices, edges, folds, and outputs from this component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRQueryComponent {
    /// The [Vid] of the root, or entry point, of the component.
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

/// Intermediate representation of a query
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRQuery {
    pub root_name: Arc<str>,

    #[serde(default, skip_serializing_if = "EdgeParameters::is_empty")]
    pub root_parameters: EdgeParameters,

    pub root_component: Arc<IRQueryComponent>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<Arc<str>, Type>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IREdge {
    pub eid: Eid,
    pub from_vid: Vid,
    pub to_vid: Vid,
    pub edge_name: Arc<str>,

    #[serde(default, skip_serializing_if = "EdgeParameters::is_empty")]
    pub parameters: EdgeParameters,

    /// Indicating if this edge is optional.
    ///
    /// Corresponds to the `@optional` directive.
    #[serde(default = "default_optional", skip_serializing_if = "is_false")]
    pub optional: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recursive: Option<Recursive>,
}

fn default_optional() -> bool {
    false
}

fn is_false(b: &bool) -> bool {
    !b
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recursive {
    pub depth: NonZeroUsize,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coerce_to: Option<Arc<str>>,
}

impl Recursive {
    pub fn new(depth: NonZeroUsize, coerce_to: Option<Arc<str>>) -> Self {
        Self { depth, coerce_to }
    }
}

/// Representation of a vertex (node) in the Trustfall intermediate
/// representation (IR).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRVertex {
    pub vid: Vid,

    /// The name of the type of the vertex as a string.
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

    #[serde(default, skip_serializing_if = "EdgeParameters::is_empty")]
    pub parameters: EdgeParameters,

    pub component: Arc<IRQueryComponent>,

    /// Tags from the directly-enclosing component whose values are needed
    /// inside this fold's component or one of its subcomponents.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imported_tags: Vec<FieldRef>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fold_specific_outputs: BTreeMap<Arc<str>, FoldSpecificFieldKind>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub post_filters: Vec<Operation<FoldSpecificFieldKind, Argument>>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FoldSpecificFieldKind {
    Count, // Represents the number of elements in an IRFold's component.
}

static NON_NULL_INT_TYPE: Lazy<Type> = Lazy::new(|| Type::new_named_type("Int", false));

impl FoldSpecificFieldKind {
    pub fn field_type(&self) -> &Type {
        match self {
            Self::Count => &NON_NULL_INT_TYPE,
        }
    }

    pub fn field_name(&self) -> &str {
        match self {
            FoldSpecificFieldKind::Count => "@fold.count",
        }
    }

    pub fn transform_suffix(&self) -> &str {
        match self {
            FoldSpecificFieldKind::Count => "count",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldSpecificField {
    // uniquely identifies the fold
    pub fold_eid: Eid,

    // used to quickly check whether the fold exists at all,
    // e.g. for "tagged parameter is optional and missing" purposes
    pub fold_root_vid: Vid,

    pub kind: FoldSpecificFieldKind,
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransformationKind {
    Count,
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldRef {
    ContextField(ContextField),
    FoldSpecificField(FoldSpecificField),
}

impl Ord for FieldRef {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (FieldRef::ContextField(f1), FieldRef::ContextField(f2)) => f1
                .vertex_id
                .cmp(&f2.vertex_id)
                .then(f1.field_name.as_ref().cmp(f2.field_name.as_ref())),
            (FieldRef::ContextField(_), FieldRef::FoldSpecificField(_)) => Ordering::Less,
            (FieldRef::FoldSpecificField(_), FieldRef::ContextField(_)) => Ordering::Greater,
            (FieldRef::FoldSpecificField(f1), FieldRef::FoldSpecificField(f2)) => {
                f1.fold_eid.cmp(&f2.fold_eid).then(f1.kind.cmp(&f2.kind))
            }
        }
    }
}

impl PartialOrd for FieldRef {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl From<ContextField> for FieldRef {
    fn from(c: ContextField) -> Self {
        Self::ContextField(c)
    }
}

impl From<FoldSpecificField> for FieldRef {
    fn from(f: FoldSpecificField) -> Self {
        Self::FoldSpecificField(f)
    }
}

impl FieldRef {
    pub fn field_type(&self) -> &Type {
        match self {
            FieldRef::ContextField(c) => &c.field_type,
            FieldRef::FoldSpecificField(f) => f.kind.field_type(),
        }
    }

    pub fn field_name(&self) -> &str {
        match self {
            FieldRef::ContextField(c) => c.field_name.as_ref(),
            FieldRef::FoldSpecificField(f) => f.kind.field_name(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Argument {
    Tag(FieldRef),
    Variable(VariableRef),
}

impl Argument {
    pub(crate) fn as_tag(&self) -> Option<&FieldRef> {
        match self {
            Argument::Tag(t) => Some(t),
            Argument::Variable(_) => None,
        }
    }
}

/// Operations that can be made in the graph.
///
/// In a Trustfall query, the `@filter` directive produces `Operation` values:
/// ```graphql
/// name @filter(op: "=", values: ["$input"])
/// ```
/// would produce the `Operation::Equals` variant, for example.
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

    /// The operation name, as it would have appeared in the `@filter` directive `op` argument.
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

    pub(crate) fn map<'a, LeftF, LeftOutT, RightF, RightOutT>(
        &'a self,
        map_left: LeftF,
        map_right: RightF,
    ) -> Operation<LeftOutT, RightOutT>
    where
        LeftOutT: Debug + Clone + PartialEq + Eq,
        RightOutT: Debug + Clone + PartialEq + Eq,
        LeftF: FnOnce(&'a LeftT) -> LeftOutT,
        RightF: FnOnce(&'a RightT) -> RightOutT,
    {
        match self {
            Operation::IsNull(left) => Operation::IsNull(map_left(left)),
            Operation::IsNotNull(left) => Operation::IsNotNull(map_left(left)),
            Operation::Equals(left, right) => Operation::Equals(map_left(left), map_right(right)),
            Operation::NotEquals(left, right) => {
                Operation::NotEquals(map_left(left), map_right(right))
            }
            Operation::LessThan(left, right) => {
                Operation::LessThan(map_left(left), map_right(right))
            }
            Operation::LessThanOrEqual(left, right) => {
                Operation::LessThanOrEqual(map_left(left), map_right(right))
            }
            Operation::GreaterThan(left, right) => {
                Operation::GreaterThan(map_left(left), map_right(right))
            }
            Operation::GreaterThanOrEqual(left, right) => {
                Operation::GreaterThanOrEqual(map_left(left), map_right(right))
            }
            Operation::Contains(left, right) => {
                Operation::Contains(map_left(left), map_right(right))
            }
            Operation::NotContains(left, right) => {
                Operation::NotContains(map_left(left), map_right(right))
            }
            Operation::OneOf(left, right) => Operation::OneOf(map_left(left), map_right(right)),
            Operation::NotOneOf(left, right) => {
                Operation::NotOneOf(map_left(left), map_right(right))
            }
            Operation::HasPrefix(left, right) => {
                Operation::HasPrefix(map_left(left), map_right(right))
            }
            Operation::NotHasPrefix(left, right) => {
                Operation::NotHasPrefix(map_left(left), map_right(right))
            }
            Operation::HasSuffix(left, right) => {
                Operation::HasSuffix(map_left(left), map_right(right))
            }
            Operation::NotHasSuffix(left, right) => {
                Operation::NotHasSuffix(map_left(left), map_right(right))
            }
            Operation::HasSubstring(left, right) => {
                Operation::HasSubstring(map_left(left), map_right(right))
            }
            Operation::NotHasSubstring(left, right) => {
                Operation::NotHasSubstring(map_left(left), map_right(right))
            }
            Operation::RegexMatches(left, right) => {
                Operation::RegexMatches(map_left(left), map_right(right))
            }
            Operation::NotRegexMatches(left, right) => {
                Operation::NotRegexMatches(map_left(left), map_right(right))
            }
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

impl<LeftT: NamedTypedValue> Operation<LeftT, Argument> {
    pub(crate) fn operand_types_valid(
        &self,
        tag_name: Option<&str>,
    ) -> Result<(), Vec<FilterTypeError>> {
        let left = self.left();
        let right = self.right();
        let left_type = left.typed();
        let right_type = right.map(|x| x.typed());

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
                if left_type.nullable() {
                    Ok(())
                } else {
                    Err(vec![FilterTypeError::NonNullableTypeFilteredForNullability(
                        self.operation_name().to_owned(),
                        left.named().to_string(),
                        left_type.to_string(),
                        matches!(self, Operation::IsNotNull(..)),
                    )])
                }
            }
            Operation::Equals(_, _) | Operation::NotEquals(_, _) => {
                // Individually, any operands are valid for equality operations.
                //
                // For the operands relative to each other, nullability doesn't matter,
                // but the rest of the type must be the same.
                let right_type = right_type.unwrap();
                if left_type.equal_ignoring_nullability(right_type) {
                    Ok(())
                } else {
                    // The right argument must be a tag at this point. If it is not a tag
                    // and the second .unwrap() below panics, then our type inference
                    // has inferred an incorrect type for the variable in the argument.
                    let tag = right.unwrap().as_tag().unwrap();

                    Err(vec![FilterTypeError::TypeMismatchBetweenTagAndFilter(
                        self.operation_name().to_string(),
                        left.named().to_string(),
                        left_type.to_string(),
                        tag_name.unwrap().to_string(),
                        tag.field_name().to_string(),
                        tag.field_type().to_string(),
                    )])
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
                if !is_base_type_orderable(left_type) {
                    errors.push(FilterTypeError::OrderingFilterOperationOnNonOrderableField(
                        self.operation_name().to_string(),
                        left.named().to_string(),
                        left_type.to_string(),
                    ));
                }

                if !is_base_type_orderable(right_type) {
                    // The right argument must be a tag at this point. If it is not a tag
                    // and the second .unwrap() below panics, then our type inference
                    // has inferred an incorrect type for the variable in the argument.
                    let tag = right.unwrap().as_tag().unwrap();

                    errors.push(FilterTypeError::OrderingFilterOperationOnNonOrderableTag(
                        self.operation_name().to_string(),
                        tag_name.unwrap().to_string(),
                        tag.field_name().to_string(),
                        tag.field_type().to_string(),
                    ));
                }

                // For the operands relative to each other, nullability doesn't matter,
                // but the types must be equal to each other.
                if !left_type.equal_ignoring_nullability(right_type) {
                    // The right argument must be a tag at this point. If it is not a tag
                    // and the second .unwrap() below panics, then our type inference
                    // has inferred an incorrect type for the variable in the argument.
                    let tag = right.unwrap().as_tag().unwrap();

                    errors.push(FilterTypeError::TypeMismatchBetweenTagAndFilter(
                        self.operation_name().to_string(),
                        left.named().to_string(),
                        left_type.to_string(),
                        tag_name.unwrap().to_string(),
                        tag.field_name().to_string(),
                        tag.field_type().to_string(),
                    ));
                }

                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(errors)
                }
            }
            Operation::Contains(_, _) | Operation::NotContains(_, _) => {
                // The left-hand operand needs to be a list, ignoring nullability.
                // The right-hand operand may be anything, if considered individually.
                let inner_type = if let Some(list) = left_type.as_list() {
                    Ok(list)
                } else {
                    Err(vec![FilterTypeError::ListFilterOperationOnNonListField(
                        self.operation_name().to_string(),
                        left.named().to_string(),
                        left_type.to_string(),
                    )])
                }?;

                let right_type = right_type.unwrap();

                // However, the type inside the left-hand list must be equal,
                // ignoring nullability, to the type of the right-hand operand.
                if inner_type.equal_ignoring_nullability(right_type) {
                    Ok(())
                } else {
                    // The right argument must be a tag at this point. If it is not a tag
                    // and the second .unwrap() below panics, then our type inference
                    // has inferred an incorrect type for the variable in the argument.
                    let tag = right.unwrap().as_tag().unwrap();

                    Err(vec![FilterTypeError::TypeMismatchBetweenTagAndFilter(
                        self.operation_name().to_string(),
                        left.named().to_string(),
                        left_type.to_string(),
                        tag_name.unwrap().to_string(),
                        tag.field_name().to_string(),
                        tag.field_type().to_string(),
                    )])
                }
            }
            Operation::OneOf(_, _) | Operation::NotOneOf(_, _) => {
                // The right-hand operand needs to be a list, ignoring nullability.
                // The left-hand operand may be anything, if considered individually.
                let right_type = right_type.unwrap();
                let inner_type = if let Some(list) = right_type.as_list() {
                    Ok(list)
                } else {
                    // The right argument must be a tag at this point. If it is not a tag
                    // and the second .unwrap() below panics, then our type inference
                    // has inferred an incorrect type for the variable in the argument.
                    let tag = right.unwrap().as_tag().unwrap();

                    Err(vec![FilterTypeError::ListFilterOperationOnNonListTag(
                        self.operation_name().to_string(),
                        tag_name.unwrap().to_string(),
                        tag.field_name().to_string(),
                        tag.field_type().to_string(),
                    )])
                }?;

                // However, the type inside the right-hand list must be equal,
                // ignoring nullability, to the type of the left-hand operand.
                if left_type.equal_ignoring_nullability(&inner_type) {
                    Ok(())
                } else {
                    // The right argument must be a tag at this point. If it is not a tag
                    // and the second .unwrap() below panics, then our type inference
                    // has inferred an incorrect type for the variable in the argument.
                    let tag = right.unwrap().as_tag().unwrap();

                    Err(vec![FilterTypeError::TypeMismatchBetweenTagAndFilter(
                        self.operation_name().to_string(),
                        left.named().to_string(),
                        left_type.to_string(),
                        tag_name.unwrap().to_string(),
                        tag.field_name().to_string(),
                        tag.field_type().to_string(),
                    )])
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
                if left_type.is_list() || left_type.base_type() != "String" {
                    errors.push(FilterTypeError::StringFilterOperationOnNonStringField(
                        self.operation_name().to_string(),
                        left.named().to_string(),
                        left_type.to_string(),
                    ));
                }

                // The right argument must be a tag at this point. If it is not a tag
                // and the second .unwrap() below panics, then our type inference
                // has inferred an incorrect type for the variable in the argument.
                let right_type = right_type.unwrap();
                if right_type.is_list() || right_type.base_type() != "String" {
                    let tag = right.unwrap().as_tag().unwrap();
                    errors.push(FilterTypeError::StringFilterOperationOnNonStringTag(
                        self.operation_name().to_string(),
                        tag_name.unwrap().to_string(),
                        tag.field_name().to_string(),
                        tag.field_type().to_string(),
                    ));
                }

                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(errors)
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextField {
    pub vertex_id: Vid,

    pub field_name: Arc<str>,

    pub field_type: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalField {
    pub field_name: Arc<str>,

    pub field_type: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariableRef {
    pub variable_name: Arc<str>,

    pub variable_type: Type,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::FieldValue;

    fn serialize_then_deserialize(value: &FieldValue) -> FieldValue {
        ron::from_str(ron::to_string(value).unwrap().as_str()).unwrap()
    }

    #[test]
    fn serialize_then_deserialize_enum() {
        let value = FieldValue::Enum("foo".into());
        let deserialized: FieldValue = serialize_then_deserialize(&value);
        assert_eq!(value, deserialized, "Serialized as: {}", ron::to_string(&value).unwrap());
    }

    #[test]
    fn serialize_then_deserialize_list() {
        let value = FieldValue::List(Arc::new([
            FieldValue::Int64(1),
            FieldValue::Int64(2),
            FieldValue::String("foo".into()),
        ]));
        let deserialized: FieldValue = serialize_then_deserialize(&value);
        assert_eq!(value, deserialized, "Serialized as: {}", ron::to_string(&value).unwrap());
    }

    #[test]
    fn serialize_then_deserialize_float() {
        let value = FieldValue::Float64(1.0);
        let deserialized: FieldValue = serialize_then_deserialize(&value);
        assert_eq!(value, deserialized, "Serialized as: {}", ron::to_string(&value).unwrap());
    }

    #[test]
    fn serialize_then_deserialize_i64() {
        let value = FieldValue::Int64(-123);
        let deserialized: FieldValue = serialize_then_deserialize(&value);
        assert_eq!(value, deserialized, "Serialized as: {}", ron::to_string(&value).unwrap());
    }

    #[test]
    fn serialize_then_deserialize_u64() {
        let value = FieldValue::Uint64((i64::MAX as u64) + 1);
        let deserialized: FieldValue = serialize_then_deserialize(&value);
        assert_eq!(value, deserialized, "Serialized as: {}", ron::to_string(&value).unwrap());
    }
}
