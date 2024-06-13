use crate::{
    graphql_query::directives::{FilterDirective, OperatorArgument},
    ir::{Argument, NamedTypedValue, Operation, Type, VariableRef, Vid},
    schema::Schema,
};

use super::{
    error::{FilterTypeError, FrontendError},
    tags::{TagHandler, TagLookupError},
    util::ComponentPath,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn make_filter_expr<LeftT: NamedTypedValue>(
    schema: &Schema,
    component_path: &ComponentPath,
    tags: &mut TagHandler<'_>,
    current_vertex_vid: Vid,
    left_operand: LeftT,
    filter_directive: &FilterDirective,
) -> Result<Operation<LeftT, Argument>, Vec<FrontendError>> {
    let filter_operation = filter_directive
        .operation
        .try_map(
            |_| Ok(left_operand.clone()),
            |arg| {
                Ok(match arg {
                    OperatorArgument::VariableRef(var_name) => Argument::Variable(VariableRef {
                        variable_name: var_name.clone(),
                        variable_type: infer_variable_type(
                            left_operand.named(),
                            left_operand.typed().clone(),
                            &filter_directive.operation,
                        )
                        .map_err(|e| *e)?,
                    }),
                    OperatorArgument::TagRef(tag_name) => {
                        let defined_tag = match tags.reference_tag(
                            tag_name.as_ref(),
                            component_path,
                            current_vertex_vid,
                        ) {
                            Ok(defined_tag) => defined_tag,
                            Err(TagLookupError::UndefinedTag(tag_name)) => {
                                return Err(FrontendError::UndefinedTagInFilter(
                                    left_operand.named().to_string(),
                                    tag_name,
                                ));
                            }
                            Err(TagLookupError::TagDefinedInsideFold(tag_name)) => {
                                return Err(FrontendError::TagUsedOutsideItsFoldedSubquery(
                                    left_operand.named().to_string(),
                                    tag_name,
                                ));
                            }
                            Err(TagLookupError::TagUsedBeforeDefinition(tag_name)) => {
                                return Err(FrontendError::TagUsedBeforeDefinition(
                                    left_operand.named().to_string(),
                                    tag_name,
                                ))
                            }
                        };

                        Argument::Tag(defined_tag.field.clone())
                    }
                })
            },
        )
        .map_err(|e| vec![e])?;

    // Get the tag name, if one was used.
    // The tag name is used to improve the diagnostics raised in case of bad query input.
    let maybe_tag_name = match filter_directive.operation.right() {
        Some(OperatorArgument::TagRef(tag_name)) => Some(tag_name.as_ref()),
        _ => None,
    };

    if let Err(e) = operand_types_valid(&filter_operation, maybe_tag_name) {
        Err(e.into_iter().map(|x| x.into()).collect())
    } else {
        Ok(filter_operation)
    }
}

fn infer_variable_type(
    property_name: &str,
    property_type: Type,
    operation: &Operation<(), OperatorArgument>,
) -> Result<Type, Box<FilterTypeError>> {
    match operation {
        Operation::Equals(..) | Operation::NotEquals(..) => {
            // Direct equality comparison.
            // If the field is nullable, then the input should be nullable too
            // so that the null valued fields can be matched.
            Ok(property_type.clone())
        }
        Operation::LessThan(..)
        | Operation::LessThanOrEqual(..)
        | Operation::GreaterThan(..)
        | Operation::GreaterThanOrEqual(..) => {
            // The null value isn't orderable relative to non-null values of its type.
            // Use a type that is structurally the same but non-null at the top level.
            //
            // Why only the top level? Consider a comparison against type [[Int]].
            // Using a "null" valued variable doesn't make sense as a comparison.
            // However, [[1], [2], null] is a valid value to use in the comparison, since
            // there are definitely values that it is smaller than or bigger than.
            Ok(property_type.with_nullability(false))
        }
        Operation::Contains(..) | Operation::NotContains(..) => {
            // To be able to check whether the property's value contains the operand,
            // the property needs to be a list. If it's not a list, this is a bad filter.
            // let value = property_type.value();
            let inner_type = if let Some(list) = property_type.as_list() {
                list
            } else {
                return Err(Box::new(FilterTypeError::ListFilterOperationOnNonListField(
                    operation.operation_name().to_string(),
                    property_name.to_string(),
                    property_type.to_string(),
                )));
            };

            // We're trying to see if a list of element contains our element, so its type
            // is whatever is inside the list -- nullable or not.
            Ok(inner_type)
        }
        Operation::OneOf(..) | Operation::NotOneOf(..) => {
            // Whatever the property's type is, the argument must be a non-nullable list of
            // the same type, so that the elements of that list may be checked for equality
            // against that property's value.
            Ok(Type::new_list_type(property_type.clone(), false))
        }
        Operation::HasPrefix(..)
        | Operation::NotHasPrefix(..)
        | Operation::HasSuffix(..)
        | Operation::NotHasSuffix(..)
        | Operation::HasSubstring(..)
        | Operation::NotHasSubstring(..)
        | Operation::RegexMatches(..)
        | Operation::NotRegexMatches(..) => {
            // Filtering operations involving strings only take non-nullable strings as inputs.
            Ok(Type::new_named_type("String", false))
        }
        Operation::IsNull(..) | Operation::IsNotNull(..) => {
            // These are unary operations, there's no place where a variable can be used.
            // There's nothing to be inferred, and this function must never be called
            // for such operations.
            unreachable!()
        }
    }
}

fn operand_types_valid<LeftT: NamedTypedValue>(
    operation: &Operation<LeftT, Argument>,
    tag_name: Option<&str>,
) -> Result<(), Vec<FilterTypeError>> {
    let left = operation.left();
    let right = operation.right();
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
    match operation {
        Operation::IsNull(_) | Operation::IsNotNull(_) => {
            validity::nullability_types_valid(operation, tag_name)
        }
        Operation::Equals(_, _) | Operation::NotEquals(_, _) => {
            validity::equality_types_valid(operation, tag_name)
        }
        Operation::LessThan(_, _)
        | Operation::LessThanOrEqual(_, _)
        | Operation::GreaterThan(_, _)
        | Operation::GreaterThanOrEqual(_, _) => {
            validity::ordering_types_valid(operation, tag_name)
        }
        Operation::Contains(_, _) | Operation::NotContains(_, _) => {
            validity::list_containment_types_valid(operation, tag_name)
        }
        Operation::OneOf(_, _) | Operation::NotOneOf(_, _) => {
            validity::bulk_equality_types_valid(operation, tag_name)
        }
        Operation::HasPrefix(_, _)
        | Operation::NotHasPrefix(_, _)
        | Operation::HasSuffix(_, _)
        | Operation::NotHasSuffix(_, _)
        | Operation::HasSubstring(_, _)
        | Operation::NotHasSubstring(_, _)
        | Operation::RegexMatches(_, _)
        | Operation::NotRegexMatches(_, _) => {
            validity::string_operation_types_valid(operation, tag_name)
        }
    }
}

mod validity {
    use crate::{
        frontend::error::FilterTypeError,
        ir::{Argument, NamedTypedValue, Operation},
    };

    pub(super) fn nullability_types_valid<LeftT: NamedTypedValue>(
        operation: &Operation<LeftT, Argument>,
        tag_name: Option<&str>,
    ) -> Result<(), Vec<FilterTypeError>> {
        let left = operation.left();
        let left_type = left.typed();

        // Checking non-nullable types for null or non-null is pointless.
        if left_type.nullable() {
            Ok(())
        } else {
            Err(vec![FilterTypeError::NonNullableTypeFilteredForNullability(
                operation.operation_name().to_owned(),
                left.named().to_string(),
                left_type.to_string(),
                matches!(operation, Operation::IsNotNull(..)),
            )])
        }
    }

    pub(super) fn equality_types_valid<LeftT: NamedTypedValue>(
        operation: &Operation<LeftT, Argument>,
        tag_name: Option<&str>,
    ) -> Result<(), Vec<FilterTypeError>> {
        let left = operation.left();
        let right = operation.right();
        let left_type = left.typed();
        let right_type = right.map(|x| x.typed());

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
                operation.operation_name().to_string(),
                left.named().to_string(),
                left_type.to_string(),
                tag_name.unwrap().to_string(),
                tag.field_name().to_string(),
                tag.field_type().to_string(),
            )])
        }
    }

    pub(super) fn ordering_types_valid<LeftT: NamedTypedValue>(
        operation: &Operation<LeftT, Argument>,
        tag_name: Option<&str>,
    ) -> Result<(), Vec<FilterTypeError>> {
        let left = operation.left();
        let right = operation.right();
        let left_type = left.typed();
        let right_type = right.map(|x| x.typed());

        // Individually, the operands' types must be non-nullable or list, recursively,
        // versions of orderable types.
        let right_type = right_type.unwrap();

        let mut errors = vec![];
        if !left_type.is_orderable() {
            errors.push(FilterTypeError::OrderingFilterOperationOnNonOrderableField(
                operation.operation_name().to_string(),
                left.named().to_string(),
                left_type.to_string(),
            ));
        }

        if !right_type.is_orderable() {
            // The right argument must be a tag at this point. If it is not a tag
            // and the second .unwrap() below panics, then our type inference
            // has inferred an incorrect type for the variable in the argument.
            let tag = right.unwrap().as_tag().unwrap();

            errors.push(FilterTypeError::OrderingFilterOperationOnNonOrderableTag(
                operation.operation_name().to_string(),
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
                operation.operation_name().to_string(),
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

    pub(super) fn list_containment_types_valid<LeftT: NamedTypedValue>(
        operation: &Operation<LeftT, Argument>,
        tag_name: Option<&str>,
    ) -> Result<(), Vec<FilterTypeError>> {
        let left = operation.left();
        let right = operation.right();
        let left_type = left.typed();
        let right_type = right.map(|x| x.typed());

        // The left-hand operand needs to be a list, ignoring nullability.
        // The right-hand operand may be anything, if considered individually.
        let inner_type = left_type.as_list().ok_or_else(|| {
            vec![FilterTypeError::ListFilterOperationOnNonListField(
                operation.operation_name().to_string(),
                left.named().to_string(),
                left_type.to_string(),
            )]
        })?;

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
                operation.operation_name().to_string(),
                left.named().to_string(),
                left_type.to_string(),
                tag_name.unwrap().to_string(),
                tag.field_name().to_string(),
                tag.field_type().to_string(),
            )])
        }
    }

    pub(super) fn bulk_equality_types_valid<LeftT: NamedTypedValue>(
        operation: &Operation<LeftT, Argument>,
        tag_name: Option<&str>,
    ) -> Result<(), Vec<FilterTypeError>> {
        let left = operation.left();
        let right = operation.right();
        let left_type = left.typed();
        let right_type = right.map(|x| x.typed());

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
                operation.operation_name().to_string(),
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
                operation.operation_name().to_string(),
                left.named().to_string(),
                left_type.to_string(),
                tag_name.unwrap().to_string(),
                tag.field_name().to_string(),
                tag.field_type().to_string(),
            )])
        }
    }

    pub(super) fn string_operation_types_valid<LeftT: NamedTypedValue>(
        operation: &Operation<LeftT, Argument>,
        tag_name: Option<&str>,
    ) -> Result<(), Vec<FilterTypeError>> {
        let left = operation.left();
        let right = operation.right();
        let left_type = left.typed();
        let right_type = right.map(|x| x.typed());

        let mut errors = vec![];

        // Both operands need to be strings, ignoring nullability.
        if left_type.is_list() || left_type.base_type() != "String" {
            errors.push(FilterTypeError::StringFilterOperationOnNonStringField(
                operation.operation_name().to_string(),
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
                operation.operation_name().to_string(),
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
