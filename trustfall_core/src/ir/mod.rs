//! Trustfall intermediate representation (IR)

use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::BTreeMap,
    fmt::Debug,
    num::NonZeroUsize,
    ops::Index,
    sync::{Arc, OnceLock},
};

use serde::{Deserialize, Serialize};

pub use self::indexed::{EdgeKind, IndexedQuery, InvalidIRQueryError, Output};
pub use self::types::Type;
pub use self::value::{FieldValue, TransparentValue};

mod indexed;
mod types;
pub mod value;

pub(crate) const TYPENAME_META_FIELD: &str = "__typename";

static TYPENAME_META_FIELD_ARC: OnceLock<Arc<str>> = OnceLock::new();

pub(crate) fn get_typename_meta_field() -> &'static Arc<str> {
    TYPENAME_META_FIELD_ARC.get_or_init(|| Arc::from(TYPENAME_META_FIELD))
}

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

/// Unique ID of a value term in a Trustfall query, such as a vertex property
/// or a computed value like the number of elements in a `@fold`.
#[doc(alias("term"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Tid(pub(crate) NonZeroUsize);

impl Tid {
    pub fn new(id: NonZeroUsize) -> Tid {
        Tid(id)
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
    pub outputs: BTreeMap<Arc<str>, FieldRef>,
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
    pub filters: Vec<Operation<OperationSubject, Argument>>,
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

    /// Outputs from this fold that are derived from fold-specific fields.
    ///
    /// All [`FieldRef`] values in the map are guaranteed to have
    /// `FieldRef.refers_to_fold_specific_field().is_some() == true`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fold_specific_outputs: BTreeMap<Arc<str>, FieldRef>,

    /// Filters that are applied on the fold as a whole.
    ///
    /// For example, as in `@fold @transform(op: "count") @filter(op: "=", value: ["$zero"])`.
    ///
    /// All [`FieldRef`] values inside each [`Operation`] within the `Vec` are guaranteed to have
    /// `FieldRef.refers_to_fold_specific_field().is_some() == true`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub post_filters: Vec<Operation<OperationSubject, Argument>>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FoldSpecificFieldKind {
    Count, // Represents the number of elements in an IRFold's component.
}

static NON_NULL_INT_TYPE: OnceLock<Type> = OnceLock::new();

impl FoldSpecificFieldKind {
    pub fn field_type(&self) -> &Type {
        match self {
            Self::Count => NON_NULL_INT_TYPE.get_or_init(|| Type::new_named_type("Int", false)),
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
    TransformedField(TransformedField),
}

impl Ord for FieldRef {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (FieldRef::ContextField(f1), FieldRef::ContextField(f2)) => f1
                .vertex_id
                .cmp(&f2.vertex_id)
                .then(f1.field_name.as_ref().cmp(f2.field_name.as_ref())),
            (
                FieldRef::ContextField(_),
                FieldRef::FoldSpecificField(_) | FieldRef::TransformedField(..),
            ) => Ordering::Less,
            (FieldRef::FoldSpecificField(_), FieldRef::ContextField(_)) => Ordering::Greater,
            (FieldRef::FoldSpecificField(..), FieldRef::TransformedField(..)) => Ordering::Less,
            (FieldRef::FoldSpecificField(f1), FieldRef::FoldSpecificField(f2)) => {
                f1.fold_eid.cmp(&f2.fold_eid).then(f1.kind.cmp(&f2.kind))
            }
            (
                FieldRef::TransformedField(..),
                FieldRef::ContextField(..) | FieldRef::FoldSpecificField(..),
            ) => Ordering::Greater,
            (FieldRef::TransformedField(f1), FieldRef::TransformedField(f2)) => f1.tid.cmp(&f2.tid),
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

impl From<TransformedField> for FieldRef {
    fn from(f: TransformedField) -> Self {
        Self::TransformedField(f)
    }
}

impl FieldRef {
    pub fn field_type(&self) -> &Type {
        match self {
            FieldRef::ContextField(c) => &c.field_type,
            FieldRef::FoldSpecificField(f) => f.kind.field_type(),
            FieldRef::TransformedField(f) => &f.field_type,
        }
    }

    pub fn field_name(&self) -> &str {
        match self {
            FieldRef::ContextField(c) => c.field_name.as_ref(),
            FieldRef::FoldSpecificField(f) => f.kind.field_name(),
            FieldRef::TransformedField(..) => todo!(),
        }
    }

    pub fn field_name_arc(&self) -> Arc<str> {
        match self {
            FieldRef::ContextField(c) => c.field_name.clone(),
            FieldRef::FoldSpecificField(f) => f.kind.field_name().into(),
            FieldRef::TransformedField(..) => todo!(),
        }
    }

    /// The vertex ID at which this reference is considered defined.
    pub fn defined_at(&self) -> Vid {
        match self {
            FieldRef::ContextField(c) => c.vertex_id,
            FieldRef::FoldSpecificField(f) => f.fold_root_vid,
            FieldRef::TransformedField(f) => match &f.value.base {
                TransformBase::ContextField(c) => c.vertex_id,
                TransformBase::FoldSpecificField(f) => f.fold_root_vid,
            },
        }
    }

    pub fn refers_to_fold_specific_field(&self) -> Option<&FoldSpecificField> {
        match self {
            FieldRef::ContextField(_) => None,
            FieldRef::FoldSpecificField(fold_specific) => Some(fold_specific),
            FieldRef::TransformedField(t) => match &t.value.base {
                TransformBase::ContextField(_) => None,
                TransformBase::FoldSpecificField(fold_specific) => Some(fold_specific),
            },
        }
    }
}

/// The right-hand side of a Trustfall operation.
///
/// In a Trustfall query, the `@filter` directive produces [`Operation`] values.
/// The right-hand side of [`Operation`] is usually [`Argument`].
///
/// For example:
/// ```graphql
/// name @filter(op: "=", value: ["$input"])
/// ```
/// produces a value like `Operation::Equals(..., Argument::Variable(...))`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Argument {
    Tag(FieldRef),
    Variable(VariableRef),
}

impl Argument {
    pub(crate) fn as_variable(&self) -> Option<&VariableRef> {
        match self {
            Argument::Variable(var) => Some(var),
            _ => None,
        }
    }

    pub(crate) fn as_tag(&self) -> Option<&FieldRef> {
        match self {
            Argument::Tag(t) => Some(t),
            Argument::Variable(_) => None,
        }
    }

    pub(crate) fn evaluate_statically<'a>(
        &self,
        query_variables: &'a BTreeMap<Arc<str>, FieldValue>,
    ) -> Option<Cow<'a, FieldValue>> {
        match self {
            Argument::Tag(..) => None,
            Argument::Variable(var) => {
                Some(Cow::Borrowed(&query_variables[var.variable_name.as_ref()]))
            }
        }
    }

    pub fn field_type(&self) -> &Type {
        match self {
            Argument::Tag(tag) => tag.field_type(),
            Argument::Variable(var) => &var.variable_type,
        }
    }
}

/// The left-hand side of a Trustfall operation.
///
/// In a Trustfall query, the `@filter` directive produces [`Operation`] values.
/// The left-hand side of [`Operation`] is usually [`OperationSubject`].
///
/// For example:
/// ```graphql
/// name @filter(op: "=", value: ["$input"])
/// ```
/// produces a value like `Operation::Equals(OperationSubject::LocalField(...), ...)`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationSubject {
    LocalField(LocalField),
    FoldSpecificField(FoldSpecificField),
    TransformedField(TransformedField),
}

impl From<LocalField> for OperationSubject {
    fn from(value: LocalField) -> Self {
        Self::LocalField(value)
    }
}

impl From<TransformedField> for OperationSubject {
    fn from(value: TransformedField) -> Self {
        Self::TransformedField(value)
    }
}

impl OperationSubject {
    pub fn refers_to_fold_specific_field(&self) -> Option<&FoldSpecificField> {
        match self {
            OperationSubject::FoldSpecificField(fold_specific) => Some(fold_specific),
            _ => None,
        }
    }

    pub fn field_type(&self) -> &Type {
        match self {
            OperationSubject::LocalField(inner) => &inner.field_type,
            OperationSubject::TransformedField(inner) => &inner.field_type,
            OperationSubject::FoldSpecificField(inner) => inner.kind.field_type(),
        }
    }
}

/// Operations that can be made in the graph.
///
/// In a Trustfall query, the `@filter` directive produces [`Operation`] values:
/// ```graphql
/// name @filter(op: "=", value: ["$input"])
/// ```
/// would produce the [`Operation::Equals`] variant, for example.
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

    pub(crate) fn map_left<'a, LeftF, LeftOutT>(
        &'a self,
        map_left: LeftF,
    ) -> Operation<LeftOutT, &RightT>
    where
        LeftOutT: Debug + Clone + PartialEq + Eq,
        LeftF: FnOnce(&'a LeftT) -> LeftOutT,
    {
        self.map(map_left, |x| x)
    }

    #[allow(dead_code)]
    pub(crate) fn map_right<'a, RightF, RightOutT>(
        &'a self,
        map_right: RightF,
    ) -> Operation<&LeftT, RightOutT>
    where
        RightOutT: Debug + Clone + PartialEq + Eq,
        RightF: FnOnce(&'a RightT) -> RightOutT,
    {
        self.map(|x| x, map_right)
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

#[non_exhaustive]
/// The outcome of a `@transform` operation applied to a vertex property or property-like value
/// such as the element count of a fold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformedField {
    pub value: Arc<TransformedValue>,

    /// The unique identifier of the transformed value this struct represents.
    pub tid: Tid,

    /// The resulting type of the value produced by this transformation.
    pub field_type: Type,
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformedValue {
    pub base: TransformBase,
    pub transforms: Vec<Transform>,
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransformBase {
    ContextField(ContextField),
    FoldSpecificField(FoldSpecificField),
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transform {
    Len,
    Abs,
    Add(Argument),
}

impl Transform {
    pub(crate) fn operation_name(&self) -> &str {
        match self {
            Self::Len => "len",
            Self::Abs => "abs",
            Self::Add(..) => "add",
        }
    }
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
