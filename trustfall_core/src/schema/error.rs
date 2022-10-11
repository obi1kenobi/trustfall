use serde::{ser::Error as SerError, Deserialize, Serialize, Serializer};

use crate::util::DisplayVec;

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum InvalidSchemaError {
    #[error("Multiple schema errors: {0}")]
    MultipleErrors(DisplayVec<InvalidSchemaError>),

    #[serde(
        skip_deserializing,
        serialize_with = "fail_serialize_schema_parse_error"
    )]
    #[error("Schema failed to parse.")]
    SchemaParseError(#[from] async_graphql_parser::Error),

    #[error(
        "Field \"{0}\" on type \"{1}\" comes from the implementation of interface \"{2}\" \
        but the field's type {3} is not compatible with the {4} type required by that interface. \
        The expected type for this field is the {4} type required by the interface, with optional \
        additional non-null constraints."
    )]
    InvalidTypeWideningOfInheritedField(String, String, String, String, String),

    #[error(
        "Field \"{0}\" on type \"{1}\" comes from the implementation of interface \"{2}\" \
        but the field's {3} parameter type {4} is not compatible with the {5} type required \
        by that interface. The expected type for this field parameter is the {5} type required by \
        the interface, optionally with non-null constraints removed if any are present."
    )]
    InvalidTypeNarrowingOfInheritedFieldParameter(String, String, String, String, String, String),

    #[error(
        "Field \"{0}\" on type \"{1}\" is missing parameter(s) that are required by \
        the implementation of interface \"{2}\" for this type: {3:?}"
    )]
    InheritedFieldMissingParameters(String, String, String, Vec<String>),

    #[error(
        "Field \"{0}\" on type \"{1}\" contains parameter(s) that are unexpected for \
        the implementation of interface \"{2}\" for this type: {3:?}"
    )]
    InheritedFieldUnexpectedParameters(String, String, String, Vec<String>),

    #[error(
        "Field \"{0}\" on type \"{1}\" accepts parameter \"{2}\" with type {3}, but gives it \
        a default value that is not valid for that type: {4}"
    )]
    InvalidDefaultValueForFieldParameter(String, String, String, String, String),

    #[error(
        "The following types have a circular implementation relationship, \
        which is not allowed: {0:?}"
    )]
    CircularImplementsRelationships(Vec<String>),

    #[error(
        "Type \"{0}\" fails to implement interface \"{2}\", which is required since \"{0}\" \
        implements \"{1}\" which in turn implements \"{2}\"."
    )]
    MissingTransitiveInterfaceImplementation(String, String, String),

    #[error(
        "Type \"{0}\" implements interface \"{1}\", but is missing field \"{2}\" of type {3} \
        which is required by that interface."
    )]
    MissingRequiredField(String, String, String, String),

    /// This may or may not be supported in the future.
    ///
    /// If supported, it will only be supported as an explicit opt-in,
    /// e.g. via an explicit directive on each type where a new ambiguity appears.
    #[error(
        "Type \"{0}\" defines field \"{1}\" of type {2}, but its origin is ambiguous because \
        multiple unrelated interfaces implemented by type \"{0}\" all define their own fields \
        by that name: {3:?}"
    )]
    AmbiguousFieldOrigin(String, String, String, Vec<String>),

    #[error(
        "Type \"{0}\" defines field \"{1}\" which is determined to be a property field because \
        of its type {2}. Property fields cannot take parameters, but this field takes parameters: \
        {3:?}"
    )]
    PropertyFieldWithParameters(String, String, String, Vec<String>),

    #[error(
        "Type \"{0}\" defines edge \"{1}\" of type {2}, which is not allowed. Edge types must be \
        vertex or list of vertex types, with optional nullability. Vertex types in two or more \
        nested lists are not supported."
    )]
    InvalidEdgeType(String, String, String),

    #[error(
        "The schema's root query type \"{0}\" defines a field \"{1}\" which is determined to \
        be a property field because of its type {2}. The root query type may only contain \
        edge fields; property fields are not allowed on the root query type."
    )]
    PropertyFieldOnRootQueryType(String, String, String),

    #[error(
        "Type \"{0}\" defines edge \"{1}\" of type {2}, but that type refers to \
        the root query type of this schema, which is not supported."
    )]
    EdgePointsToRootQueryType(String, String, String),

    #[error(
        "Type \"{0}\" defines field \"{1}\", but the field prefix \"__\" is reserved \
        for internal use and cannot be used in schemas."
    )]
    ReservedFieldName(String, String),

    #[error(
        "Type \"{0}\" uses the prefix \"__\" which is reserved \
        for internal use and cannot be used in schemas."
    )]
    ReservedTypeName(String),
}

impl From<Vec<InvalidSchemaError>> for InvalidSchemaError {
    fn from(v: Vec<InvalidSchemaError>) -> Self {
        assert!(!v.is_empty());
        if v.len() == 1 {
            v.into_iter().next().unwrap()
        } else {
            Self::MultipleErrors(DisplayVec(v))
        }
    }
}

fn fail_serialize_schema_parse_error<S: Serializer>(
    _: &async_graphql_parser::Error,
    _: S,
) -> Result<S::Ok, S::Error> {
    Err(S::Error::custom(
        "cannot serialize SchemaParseError error variant",
    ))
}
