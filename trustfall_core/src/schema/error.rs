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
        "The following types have a circular implementation relationship, \
        which is not allowed: {0:?}"
    )]
    CircularImplementsRelationships(Vec<String>),

    #[error(
        "Type \"{0}\" implements interface \"{1}\", but is missing field \"{2}\" of type {3} \
        which is required by that interface."
    )]
    MissingRequiredField(String, String, String, String),
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
