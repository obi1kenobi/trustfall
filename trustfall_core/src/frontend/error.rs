use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    ir::{FieldValue, Type},
    util::DisplayVec,
};

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum FrontendError {
    #[error("{0}")]
    MultipleErrors(DisplayVec<FrontendError>),

    #[error("{0}")]
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

    #[error(
        "Tagged fields with an applied @transform must explicitly specify the tag name, like this: \
        @tag(name: \"some_name\"). Affected field: {0}"
    )]
    ExplicitTagNameRequired(String),

    #[error("Incompatible types encountered in @filter: {0}")]
    FilterTypeError(#[from] FilterTypeError),

    #[error("Found {0} applied to \"{1}\" property, which is not supported since that directive can only be applied to edges.")]
    UnsupportedDirectiveOnProperty(String, String),

    #[error("Found an edge with an @output directive, this is not supported: {0}")]
    UnsupportedEdgeOutput(String),

    #[error("Found an edge with an unsupported @filter directive: {0}")]
    UnsupportedEdgeFilter(String),

    #[error("Found an edge with an unsupported @tag directive: {0}")]
    UnsupportedEdgeTag(String),

    #[error("Found an unsupported {1} directive on an edge with @fold: {0}")]
    UnsupportedDirectiveOnFoldedEdge(String, String),

    #[error("Missing required edge parameter \"{0}\" on edge {1}")]
    MissingRequiredEdgeParameter(String, String),

    #[error("Unexpected edge parameter \"{0}\" on edge {1}")]
    UnexpectedEdgeParameter(String, String),

    #[error(
        "Invalid value for edge parameter \"{0}\" on edge {1}. \
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

    #[error("The query failed to validate against the schema: {0}")]
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
        "Filter operation \"{0}\" is applied on non-nullable {1}. \
        The filter's result would always be {2}. Please rewrite the query to avoid this filter."
    )]
    NonNullableTypeFilteredForNullability(String, String, bool),

    #[error(
        "Filter operation \"{0}\" is comparing values with incompatible type: {1} versus {2}."
    )]
    TypeMismatchBetweenFilterSubjectAndArgument(String, String, String),

    #[error(
        "Filter operation \"{0}\" can only be applied to orderable values, but is applied to {1} \
        which does not support ordering comparisons."
    )]
    OrderingFilterOperationOnNonOrderableSubject(String, String),

    #[error(
        "Filter operation \"{0}\" requires an argument that supports ordering comparisons, \
        but is being used with non-orderable {1}."
    )]
    OrderingFilterOperationWithNonOrderableArgument(String, String),

    #[error(
        "Filter operation \"{0}\" can only be applied to string values, but is applied to {1} \
        which is not a string."
    )]
    StringFilterOperationOnNonStringSubject(String, String),

    #[error(
        "Filter operation \"{0}\" requires an argument of string type, but is being used \
        with non-string {1}."
    )]
    StringFilterOperationOnNonStringArgument(String, String),

    #[error(
        "Filter operation \"{0}\" can only be applied to list values, but is applied to {1} \
        which is not a list."
    )]
    ListFilterOperationOnNonListSubject(String, String),

    #[error(
        "Filter operation \"{0}\" requires an argument of list type, but is being used \
        with non-list {1}."
    )]
    ListFilterOperationOnNonListArgument(String, String),
}

impl FilterTypeError {
    fn represent_property_and_type(property_name: &str, property_type: &Type) -> String {
        format!("property \"{property_name}\" of type \"{property_type}\"")
    }

    fn represent_tag_name_and_type(tag_name: &str, tag_type: &Type) -> String {
        format!("tag \"{tag_name}\" of type \"{tag_type}\"")
    }

    pub(crate) fn non_nullable_property_with_nullability_filter(
        filter_operator: &str,
        property_name: &str,
        property_type: &Type,
        filter_outcome: bool,
    ) -> Self {
        Self::NonNullableTypeFilteredForNullability(
            filter_operator.to_string(),
            Self::represent_property_and_type(property_name, property_type),
            filter_outcome,
        )
    }

    pub(crate) fn type_mismatch_between_property_and_tag(
        filter_operator: &str,
        property_name: &str,
        property_type: &Type,
        tag_name: &str,
        tag_type: &Type,
    ) -> Self {
        Self::TypeMismatchBetweenFilterSubjectAndArgument(
            filter_operator.to_string(),
            Self::represent_property_and_type(property_name, property_type),
            Self::represent_tag_name_and_type(tag_name, tag_type),
        )
    }

    pub(crate) fn non_orderable_property_with_ordering_filter(
        filter_operator: &str,
        property_name: &str,
        property_type: &Type,
    ) -> Self {
        Self::OrderingFilterOperationOnNonOrderableSubject(
            filter_operator.to_string(),
            Self::represent_property_and_type(property_name, property_type),
        )
    }

    pub(crate) fn non_orderable_tag_argument_to_ordering_filter(
        filter_operator: &str,
        tag_name: &str,
        tag_type: &Type,
    ) -> Self {
        Self::OrderingFilterOperationWithNonOrderableArgument(
            filter_operator.to_string(),
            Self::represent_tag_name_and_type(tag_name, tag_type),
        )
    }

    pub(crate) fn non_string_property_with_string_filter(
        filter_operator: &str,
        property_name: &str,
        property_type: &Type,
    ) -> Self {
        Self::StringFilterOperationOnNonStringSubject(
            filter_operator.to_string(),
            Self::represent_property_and_type(property_name, property_type),
        )
    }

    pub(crate) fn non_string_tag_argument_to_string_filter(
        filter_operator: &str,
        tag_name: &str,
        tag_type: &Type,
    ) -> Self {
        Self::StringFilterOperationOnNonStringArgument(
            filter_operator.to_string(),
            Self::represent_tag_name_and_type(tag_name, tag_type),
        )
    }

    pub(crate) fn non_list_property_with_list_filter(
        filter_operator: &str,
        property_name: &str,
        property_type: &Type,
    ) -> Self {
        Self::ListFilterOperationOnNonListSubject(
            filter_operator.to_string(),
            Self::represent_property_and_type(property_name, property_type),
        )
    }

    pub(crate) fn non_list_tag_argument_to_list_filter(
        filter_operator: &str,
        tag_name: &str,
        tag_type: &Type,
    ) -> Self {
        Self::ListFilterOperationOnNonListArgument(
            filter_operator.to_string(),
            Self::represent_tag_name_and_type(tag_name, tag_type),
        )
    }
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
