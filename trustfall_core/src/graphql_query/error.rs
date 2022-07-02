use async_graphql_parser::Pos;
use async_graphql_value::Value;
use serde::{ser::Error as SerError, Deserialize, Serialize, Serializer};

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum ParseError {
    #[error("Found unrecognized directive {0}")]
    UnrecognizedDirective(String, Pos),

    #[error("Found directive in unsupported position {0}: {1}")]
    UnsupportedDirectivePosition(String, String, Pos),

    #[error("Directive {0} missing required argument {1}")]
    MissingRequiredDirectiveArgument(String, String, Pos),

    #[error("Directive {0} received unrecognized argument {1}")]
    UnrecognizedDirectiveArgument(String, String, Pos),

    #[error("Directive {0} received duplicated argument {1}")]
    DuplicatedDirectiveArgument(String, String, Pos),

    #[error("Directive {0} received value of inappropriate type for argument {1}")]
    InappropriateTypeForDirectiveArgument(String, String, Pos),

    #[error("Field {0} received an invalid value for argument {1}: {2}")]
    InvalidFieldArgument(String, String, Value, Pos),

    #[error("Input contains non-inline fragments, this is not supported")]
    DocumentContainsNonInlineFragments(Pos),

    #[error("Input contains multiple operation blocks, this is not supported")]
    MultipleOperationsInDocument(Pos),

    #[error(
        "Input contains multiple root vertices, which is not supported. \
        Please make sure that only a single field inside the outer-most curly braces is expanded."
    )]
    MultipleQueryRoots(Pos),

    #[error("Found {0} instead of a root vertex, which is not supported.")]
    UnsupportedQueryRoot(String, Pos),

    #[error("Found directive {0} applied on or outside the root vertex, which is not supported ")]
    DirectiveNotInsideQueryRoot(String, Pos),

    #[error("Input is not a query operation")]
    DocumentNotAQuery(Pos),

    #[error("Unrecognized filter operator: {0}")]
    UnsupportedFilterOperator(String, Pos),

    #[error("Unrecognized transform operator: {0}")]
    UnsupportedTransformOperator(String, Pos),

    #[error("Specified output name \"{0}\" contains invalid characters: {1:?}")]
    InvalidOutputName(String, Vec<char>, Pos),

    #[error("Specified tag name \"{0}\" contains invalid characters: {1:?}")]
    InvalidTagName(String, Vec<char>, Pos),

    #[serde(
        skip_deserializing,
        serialize_with = "fail_serialize_invalid_graphql_error"
    )]
    #[error("Input is not valid GraphQL: {0}")]
    InvalidGraphQL(async_graphql_parser::Error),

    #[error("Unsupported syntax feature found: {0}")]
    UnsupportedSyntax(String, Pos),

    #[error("Nested type coercion found. Please merge the type coercion blocks so that coercion is only performed once.")]
    NestedTypeCoercion(Pos),

    #[error("Found a type coercion with sibling fields. Please move those fields inside the type coercion.")]
    TypeCoercionWithSiblingFields(Pos),

    #[error("Directive \"{0}\" is applied more than once, this is not supported.")]
    UnsupportedDuplicatedDirective(String, Pos),

    #[error("Edge {1} specifies a duplicated parameter {0}")]
    DuplicatedEdgeParameter(String, String, Pos),

    #[error("Unexpected error: {0}")]
    OtherError(String, Pos),
}

fn fail_serialize_invalid_graphql_error<S: Serializer>(
    _: &async_graphql_parser::Error,
    _: S,
) -> Result<S::Ok, S::Error> {
    Err(S::Error::custom(
        "cannot serialize InvalidGraphQL error variant",
    ))
}

impl From<async_graphql_parser::Error> for ParseError {
    fn from(e: async_graphql_parser::Error) -> Self {
        Self::InvalidGraphQL(e)
    }
}
