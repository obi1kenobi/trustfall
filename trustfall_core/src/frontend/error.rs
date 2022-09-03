use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ir::FieldValue, util::DisplayVec};

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum FrontendError {
    #[error("Multiple errors: {0}")]
    MultipleErrors(DisplayVec<FrontendError>),

    #[error("Query failed to parse.")]
    ParseError(#[from] crate::graphql_query::error::ParseError),

    #[error("Filter on property name \"{0}\" uses undefined tag: %{1}")]
    UndefinedTagInFilter(String, String),

    #[error(
        "Filter on property name \"{0}\" uses tag \"{1}\" which is not yet defined at that point \
        in the query. Please reorder the query components so that the @tag directive \
        comes before all uses of its tagged value."
    )]
    TagUsedBeforeDefinition(String, String),

    #[error(
        "Tag \"{1}\" is defined within a @fold but is used outside that @fold in a filter on \
        property name \"{0}\". This is not supported; if possible, please consider reorganizing \
        the query so that the tagged values are captured outside the @fold and \
        their use in @filter moves inside the @fold."
    )]
    TagUsedOutsideItsFoldedSubquery(String, String),

    #[error(
        "One or more tags were defined in the query but were never used. Please remove these \
        unused @tag directives. Unused tag names: {0:?}"
    )]
    UnusedTags(Vec<String>),

    #[error("Multiple fields are being output under the same name: {0:?}")]
    MultipleOutputsWithSameName(DuplicatedNamesConflict),

    #[error("Multiple fields have @tag directives with the same name: {0}")]
    MultipleTagsWithSameName(String),

    #[error("Incompatible types encountered in @filter.")]
    FilterTypeError(#[from] FilterTypeError),

    #[error("Found an edge with an @output directive, this is not supported: {0}")]
    UnsupportedEdgeOutput(String),

    #[error("Found an edge with an unsupported @filter directive: {0}")]
    UnsupportedEdgeFilter(String),

    #[error("Found an unsupported {1} directive on an edge with @fold: {0}")]
    UnsupportedDirectiveOnFoldedEdge(String, String),

    #[error("Missing required edge parameter {0} on edge {1}")]
    MissingRequiredEdgeParameter(String, String),

    #[error("Unexpected edge parameter {0} on edge {1}")]
    UnexpectedEdgeParameter(String, String),

    #[error(
        "Invalid value for edge parameter {0} on edge {1}. \
        Expected a value of type {2}, but got: {3:?}"
    )]
    InvalidEdgeParameterType(String, String, String, FieldValue),

    #[error(
        "Invalid use of @recurse on edge \"{0}\". That edge cannot be recursed since it connects \
        two unrelated vertex types: {1} {2}"
    )]
    RecursingNonRecursableEdge(String, String, String),

    #[error(
        "Invalid use of @recurse on edge \"{0}\" in its current location. \
        The edge is currently recursed starting from a vertex of type {1} and points to \
        a vertex of type {2}, which is a subtype of {1}. Recursion to a subtype is not allowed \
        since the starting vertex might not match that type. To ensure the starting vertex matches \
        the edge's destination type, you could use a type coercion like: ... on {2}"
    )]
    RecursionToSubtype(String, String, String),

    // This error type may or may not be reachable in practice.
    // At the time of writing, schemas containing fields with ambiguous origin are disallowed,
    // though they may be allowed in the future. If they are allowed, then this error is reachable.
    #[error("Edge {0} has an ambiguous origin, and cannot be used for recursion.")]
    AmbiguousOriginEdgeRecursion(String),

    #[error(
        "Edge \"{0}\" is used for recursion that requires multiple implicit coercions, \
        which is currently not supported."
    )]
    EdgeRecursionNeedingMultipleCoercions(String),

    #[error("Meta field \"{0}\" is a property but the query uses it as an edge.")]
    PropertyMetaFieldUsedAsEdge(String),

    #[error("The query failed to validate against the schema.")]
    ValidationError(#[from] ValidationError),

    #[error("Unexpected error: {0}")]
    OtherError(String),
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum FilterTypeError {
    #[error(
        "Variable \"{0}\" is used in multiple places in the query that require values of \
        incompatible types \"{1}\" and \"{2}\". Please split up the uses that require different \
        types into separate variables."
    )]
    IncompatibleVariableTypeRequirements(String, String, String),

    #[error(
        "Filter operation \"{0}\" unexpectedly used on non-nullable field \"{1}\" of type \"{2}\". \
        The filter's result would always be {3}. Please rewrite the query to avoid this filter."
    )]
    NonNullableTypeFilteredForNullability(String, String, String, bool),

    #[error(
        "Type mismatch in @filter operation \"{0}\" between field \"{1}\" of type \"{2}\" \
        and tag \"{3}\" representing field \"{4}\" of type \"{5}\""
    )]
    TypeMismatchBetweenTagAndFilter(String, String, String, String, String, String),

    #[error(
        "Non-orderable field \"{1}\" (type \"{2}\") used with @filter operation \"{0}\" which \"
        only supports orderable types."
    )]
    OrderingFilterOperationOnNonOrderableField(String, String, String),

    #[error(
        "Tag \"{1}\" represents non-orderable field \"{2}\" (type \"{3}\"), but is used with \
        @filter operation \"{0}\" which only supports orderable types."
    )]
    OrderingFilterOperationOnNonOrderableTag(String, String, String, String),

    #[error(
        "Non-string field \"{1}\" (type \"{2}\") used with @filter operation \"{0}\" which \"
        only supports strings."
    )]
    StringFilterOperationOnNonStringField(String, String, String),

    #[error(
        "Tag \"{1}\" represents non-string field \"{2}\" (type \"{3}\"), but is used with @filter \
        operation \"{0}\" which only supports strings."
    )]
    StringFilterOperationOnNonStringTag(String, String, String, String),

    #[error(
        "Non-list field \"{1}\" (type \"{2}\") used with @filter operation \"{0}\" which can \"
        only be used on lists."
    )]
    ListFilterOperationOnNonListField(String, String, String),

    #[error(
        "Tag \"{1}\" represents non-list field \"{2}\" (type \"{3}\"), but is used with @filter \
        operation \"{0}\" which requires a list type."
    )]
    ListFilterOperationOnNonListTag(String, String, String, String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct DuplicatedNamesConflict {
    // duplicate output name -> vec (type name, field name) being output under that name
    // TODO: it may be better to replace the type name with the edge used to get to the type,
    //       and ideally also add span / Pos info pointing to the specific thing that's the problem.
    pub duplicates: BTreeMap<String, Vec<(String, String)>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum ValidationError {
    #[error("The referenced path does not exist in the schema: {0:?}")]
    NonExistentPath(Vec<String>),

    #[error("The referenced type does not exist in the schema: {0}")]
    NonExistentType(String),

    #[error(
        "Attempted to coerce type {0} into type {1}, but type {0} is not an interface. \
        Only interface types may be coerced to subtypes."
    )]
    CannotCoerceNonInterfaceType(String, String),

    #[error(
        "Attempted to coerce type {0} into type {1}, which is not a subtype of {0}. \
        This is not allowed."
    )]
    CannotCoerceToUnrelatedType(String, String),
}

impl From<async_graphql_parser::Error> for FrontendError {
    fn from(e: async_graphql_parser::Error) -> Self {
        Self::ParseError(e.into())
    }
}

impl From<Vec<FrontendError>> for FrontendError {
    fn from(v: Vec<FrontendError>) -> Self {
        assert!(!v.is_empty());
        if v.len() == 1 {
            v.into_iter().next().unwrap()
        } else {
            Self::MultipleErrors(DisplayVec(v))
        }
    }
}

// HACK: necessary to minimize immediate breakage,
//       should be removed as more refined error variants are added
impl From<&str> for FrontendError {
    fn from(s: &str) -> Self {
        Self::OtherError(s.to_owned())
    }
}
impl From<String> for FrontendError {
    fn from(s: String) -> Self {
        Self::OtherError(s)
    }
}
