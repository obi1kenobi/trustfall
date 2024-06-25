use std::{collections::BTreeMap, fmt::Write};

use serde::{Deserialize, Serialize};

use crate::{
    graphql_query::directives::{TransformDirective, TransformOp},
    ir::{
        Argument, FieldValue, FoldSpecificField, FoldSpecificFieldKind, OperationSubject,
        Transform, TransformBase, TransformedField, Type,
    },
    util::DisplayVec,
};

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum FrontendError {
    #[error("{0}")]
    MultipleErrors(DisplayVec<FrontendError>),

    #[error("{0}")]
    ParseError(#[from] crate::graphql_query::error::ParseError),

    #[error("Filter on {0} uses undefined tag: %{1}")]
    UndefinedTagInFilter(String, String),

    #[error("Transform on {0} uses undefined tag: %{1}")]
    UndefinedTagInTransform(String, String),

    #[error(
        "An operation on {0} uses tag \"{1}\" which is not yet defined at that point \
        in the query. Please reorder the query components so that the @tag directive \
        comes before all uses of its tagged value."
    )]
    TagUsedBeforeDefinition(String, String),

    #[error(
        "Tag \"{1}\" is defined within a @fold but is used outside that @fold in a filter on \
        {0}. This is not supported; if possible, please consider reorganizing \
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
        @tag(name: \"some_name\"). Affected location: {0}"
    )]
    ExplicitTagNameRequired(String),

    #[error("Incompatible types encountered in @filter: {0}")]
    FilterTypeError(#[from] FilterTypeError),

    #[error("Incompatible types encountered in @transform: {0}")]
    TransformTypeError(#[from] TransformTypeError),

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
    #[error("Edge \"{0}\" has an ambiguous origin, and cannot be used for recursion.")]
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

impl FrontendError {
    #[inline]
    pub(super) fn represent_property(property_name: &str) -> String {
        format!("property \"{property_name}\"")
    }

    #[inline]
    pub(super) fn represent_fold_specific_field(kind: &FoldSpecificFieldKind) -> String {
        format!("transformed field \"{}\"", kind.field_name())
    }

    #[inline]
    fn represent_subject(subject: &OperationSubject) -> String {
        match subject {
            OperationSubject::LocalField(field) => {
                let property_name = &field.field_name;
                Self::represent_property(property_name)
            }
            OperationSubject::TransformedField(field) => {
                let mut buf = String::with_capacity(32);
                write_name_of_transformed_field(&mut buf, field);
                buf
            }
            OperationSubject::FoldSpecificField(field) => {
                Self::represent_fold_specific_field(&field.kind)
            }
        }
    }

    pub(super) fn undefined_tag_in_filter(
        subject: &OperationSubject,
        tag_name: impl Into<String>,
    ) -> Self {
        Self::UndefinedTagInFilter(Self::represent_subject(subject), tag_name.into())
    }

    pub(super) fn tag_used_before_definition(
        subject: &OperationSubject,
        tag_name: impl Into<String>,
    ) -> Self {
        Self::TagUsedBeforeDefinition(Self::represent_subject(subject), tag_name.into())
    }

    pub(super) fn tag_used_outside_its_folded_subquery(
        subject: &OperationSubject,
        tag_name: impl Into<String>,
    ) -> Self {
        Self::TagUsedOutsideItsFoldedSubquery(Self::represent_subject(subject), tag_name.into())
    }

    pub(super) fn explicit_tag_name_required(subject: &OperationSubject) -> Self {
        Self::ExplicitTagNameRequired(Self::represent_subject(subject))
    }
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
    #[inline]
    fn represent_property_and_type(property_name: &str, property_type: &Type) -> String {
        format!("property \"{property_name}\" of type \"{property_type}\"")
    }

    #[inline]
    fn represent_tag_name_and_type(tag_name: &str, tag_type: &Type) -> String {
        format!("tag \"{tag_name}\" of type \"{tag_type}\"")
    }

    #[inline]
    fn represent_variable_name_and_type(var_name: &str, var_type: &Type) -> String {
        format!("variable \"{var_name}\" of type \"{var_type}\"")
    }

    #[inline]
    fn represent_fold_specific_field(field: &FoldSpecificField) -> String {
        let field_name = field.kind.field_name();
        let field_type = field.kind.field_type();
        format!("transformed field \"{field_name}\" of type \"{field_type}\"")
    }

    #[inline]
    fn represent_transformed_field(field: &TransformedField) -> String {
        let mut buf = String::with_capacity(64);

        write_name_of_transformed_field(&mut buf, field);

        let field_type = &field.field_type;
        write!(buf, " of type \"{field_type}\"").expect("write failed");
        buf
    }

    fn represent_subject(subject: &OperationSubject) -> String {
        match subject {
            OperationSubject::LocalField(field) => {
                Self::represent_property_and_type(&field.field_name, &field.field_type)
            }
            OperationSubject::TransformedField(field) => Self::represent_transformed_field(field),
            OperationSubject::FoldSpecificField(field) => {
                Self::represent_fold_specific_field(field)
            }
        }
    }

    /// Represent a filter argument as a human-readable string suitable for use in an error message.
    /// Tag arguments don't carry a name inside the [`Argument`] type, so we look up and supply
    /// the tag name separately if needed.
    fn represent_argument(argument: &Argument, tag_name: Option<&str>) -> String {
        match argument {
            Argument::Tag(tag) => Self::represent_tag_name_and_type(
                tag_name.expect("tag argument without a name"),
                tag.field_type(),
            ),
            Argument::Variable(var) => {
                Self::represent_variable_name_and_type(&var.variable_name, &var.variable_type)
            }
        }
    }

    pub(crate) fn non_nullable_subject_with_nullability_filter(
        filter_operator: &str,
        subject: &OperationSubject,
        filter_outcome: bool,
    ) -> Self {
        Self::NonNullableTypeFilteredForNullability(
            filter_operator.to_string(),
            Self::represent_subject(subject),
            filter_outcome,
        )
    }

    pub(crate) fn type_mismatch_between_subject_and_argument(
        filter_operator: &str,
        subject: &OperationSubject,
        argument: &Argument,
        tag_name: Option<&str>,
    ) -> Self {
        Self::TypeMismatchBetweenFilterSubjectAndArgument(
            filter_operator.to_string(),
            Self::represent_subject(subject),
            Self::represent_argument(argument, tag_name),
        )
    }

    pub(crate) fn non_orderable_subject_with_ordering_filter(
        filter_operator: &str,
        subject: &OperationSubject,
    ) -> Self {
        Self::OrderingFilterOperationOnNonOrderableSubject(
            filter_operator.to_string(),
            Self::represent_subject(subject),
        )
    }

    pub(crate) fn non_orderable_argument_to_ordering_filter(
        filter_operator: &str,
        argument: &Argument,
        tag_name: Option<&str>,
    ) -> Self {
        Self::OrderingFilterOperationWithNonOrderableArgument(
            filter_operator.to_string(),
            Self::represent_argument(argument, tag_name),
        )
    }

    pub(crate) fn non_string_subject_with_string_filter(
        filter_operator: &str,
        subject: &OperationSubject,
    ) -> Self {
        Self::StringFilterOperationOnNonStringSubject(
            filter_operator.to_string(),
            Self::represent_subject(subject),
        )
    }

    pub(crate) fn non_string_argument_to_string_filter(
        filter_operator: &str,
        argument: &Argument,
        tag_name: Option<&str>,
    ) -> Self {
        Self::StringFilterOperationOnNonStringArgument(
            filter_operator.to_string(),
            Self::represent_argument(argument, tag_name),
        )
    }

    pub(crate) fn non_list_subject_with_list_filter(
        filter_operator: &str,
        subject: &OperationSubject,
    ) -> Self {
        Self::ListFilterOperationOnNonListSubject(
            filter_operator.to_string(),
            Self::represent_subject(subject),
        )
    }

    pub(crate) fn non_list_argument_to_list_filter(
        filter_operator: &str,
        argument: &Argument,
        tag_name: Option<&str>,
    ) -> Self {
        Self::ListFilterOperationOnNonListArgument(
            filter_operator.to_string(),
            Self::represent_argument(argument, tag_name),
        )
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum TransformTypeError {
    #[error(
        "Transform operation \"{0}\" may only be applied to edges that are marked @fold, \
        but is used on {1}.{2}"
    )]
    FoldSpecificTransformUsedOnProperty(String, String, String),

    #[error(
        "Folded edge \"{0}\" has more than one @transform(op: \"count\") directive applied to it, \
        which is not allowed. Please remove all but the first such directive after the @fold."
    )]
    DuplicatedCountTransformOnEdge(String),

    #[error(
        "Transform operation \"{0}\" is not supported on edges, but was applied to edge \"{1}\".{2}"
    )]
    UnsupportedTransformUsedOnEdge(String, String, String),

    #[error("Found a @transform directive applied to edge \"{0}\" which is not marked @fold, and therefore cannot be transformed.{1}")]
    CannotTransformEdgeWithoutFold(String, String),

    #[error(
        "Transform operation \"{0}\" can only be applied on {1}, but was used on {2} of incompatible type \"{3}\"."
    )]
    TypeMismatchBetweenTransformOperationAndSubject(String, String, String, String),

    #[error(
        "Transform operation \"{0}\" requires an argument of type {1}, but was used with an argument of incompatible type \"{2}\"."
    )]
    TypeMismatchBetweenTransformOperationAndArgument(String, String, String),
}

impl TransformTypeError {
    pub(crate) fn fold_specific_transform_on_propertylike_value(
        transform_op: &str,
        property_name: &str,
        transforms_so_far: &[Transform],
        type_so_far: &Type,
    ) -> Self {
        let base_name = if transforms_so_far.is_empty() {
            FrontendError::represent_property(property_name)
        } else {
            let mut buf = String::with_capacity(16);
            write_name_of_transformed_field_by_parts(&mut buf, property_name, transforms_so_far);
            buf
        };

        let advice = if type_so_far.is_list() {
            format!(
                " To get the number of elements in the list value here (type \"{type_so_far}\"), \
                use the \"len\" transform operation instead.",
            )
        } else {
            String::new()
        };

        Self::FoldSpecificTransformUsedOnProperty(transform_op.to_string(), base_name, advice)
    }

    pub(crate) fn add_errors_for_transform_used_on_unfolded_edge(
        edge_name: &str,
        transform_directive: &TransformDirective,
        errors: &mut Vec<FrontendError>,
    ) {
        match &transform_directive.kind {
            TransformOp::Count => {
                // The user probably just forgot `@fold` on the edge, let's suggest that advice.
                errors.push(
                    Self::CannotTransformEdgeWithoutFold(
                        edge_name.to_string(),
                        " Did you mean to apply @fold to the edge before the @transform directive?"
                            .into(),
                    )
                    .into(),
                );
            }
            _ => {
                // The user is using a transform operation that is inappropriate for edges,
                // regardless of whether `@fold` is used or not.
                // They might have meant to apply the `@transform` on a property instead.
                // We should suggest that instead of suggesting adding `@fold` on the edge.
                errors.push(
                    Self::CannotTransformEdgeWithoutFold(edge_name.to_string(), "".into()).into(),
                );
                errors.push(
                    Self::UnsupportedTransformUsedOnEdge(
                        transform_directive.kind.op_name().to_string(),
                        edge_name.to_string(),
                        " Did you mean to apply the @transform directive to some property instead?"
                            .to_string(),
                    )
                    .into(),
                );
            }
        }
    }

    pub(crate) fn unsupported_transform_used_on_folded_edge(
        edge_name: &str,
        transform_op: &str,
    ) -> Self {
        Self::UnsupportedTransformUsedOnEdge(
            transform_op.to_string(),
            edge_name.to_string(),
            " Did you mean to use @transform(op: \"count\") instead?".to_string(),
        )
    }

    pub(crate) fn duplicated_count_transform_on_folded_edge(edge_name: &str) -> Self {
        Self::DuplicatedCountTransformOnEdge(edge_name.to_string())
    }

    pub(crate) fn operation_requires_list_type_subject(op: &str, subject_representation: String, subject_type: &Type) -> Self {
        Self::TypeMismatchBetweenTransformOperationAndSubject(
            op.to_string(),
            "list-typed values".to_string(),
            subject_representation,
            subject_type.to_string(),
        )
    }

    pub(crate) fn operation_requires_different_type_subject(op: &str, required_type: &Type, subject_representation: String, subject_type: &Type) -> Self {
        Self::TypeMismatchBetweenTransformOperationAndSubject(
            op.to_string(),
            format!("values of type \"{required_type}\""),
            subject_representation,
            subject_type.to_string(),
        )
    }

    pub(crate) fn operation_requires_different_choice_of_type_subject(op: &str, required_type_a: &Type, required_type_b: &Type, subject_representation: String, subject_type: &Type) -> Self {
        Self::TypeMismatchBetweenTransformOperationAndSubject(
            op.to_string(),
            format!("values of type \"{required_type_a}\" or \"{required_type_b}\""),
            subject_representation,
            subject_type.to_string(),
        )
    }
}

fn write_name_of_transformed_field(buf: &mut String, field: &TransformedField) {
    let base_name = match &field.value.base {
        TransformBase::ContextField(c) => &c.field_name,
        TransformBase::FoldSpecificField(f) => f.kind.field_name(),
    };

    write_name_of_transformed_field_by_parts(buf, base_name, &field.value.transforms);
}

pub(super) fn write_name_of_transformed_field_by_parts(
    buf: &mut String,
    base_name: &str,
    transforms: &[Transform],
) {
    buf.push_str("transformed field \"");

    buf.push_str(base_name);
    for transform in transforms {
        buf.push('.');
        buf.push_str(transform.operation_output_name());
    }
    buf.push('"');
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
