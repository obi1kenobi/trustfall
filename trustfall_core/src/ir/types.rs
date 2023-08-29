use std::fmt::Debug;

use super::{
    ty::Type, Argument, ContextField, FieldRef, FieldValue, FoldSpecificField,
    FoldSpecificFieldKind, LocalField, VariableRef,
};

pub trait NamedTypedValue: Debug + Clone + PartialEq + Eq {
    fn typed(&self) -> &Type;

    fn named(&self) -> &str;
}

impl NamedTypedValue for LocalField {
    fn typed(&self) -> &Type {
        &self.field_type
    }

    fn named(&self) -> &str {
        self.field_name.as_ref()
    }
}

impl NamedTypedValue for ContextField {
    fn typed(&self) -> &Type {
        &self.field_type
    }

    fn named(&self) -> &str {
        self.field_name.as_ref()
    }
}

impl NamedTypedValue for FoldSpecificField {
    fn typed(&self) -> &Type {
        self.kind.field_type()
    }

    fn named(&self) -> &str {
        self.kind.field_name()
    }
}

impl NamedTypedValue for FoldSpecificFieldKind {
    fn typed(&self) -> &Type {
        self.field_type()
    }

    fn named(&self) -> &str {
        self.field_name()
    }
}

impl NamedTypedValue for VariableRef {
    fn typed(&self) -> &Type {
        &self.variable_type
    }

    fn named(&self) -> &str {
        &self.variable_name
    }
}

impl NamedTypedValue for FieldRef {
    fn typed(&self) -> &Type {
        match self {
            FieldRef::ContextField(c) => c.typed(),
            FieldRef::FoldSpecificField(f) => f.kind.typed(),
        }
    }

    fn named(&self) -> &str {
        match self {
            FieldRef::ContextField(c) => c.named(),
            FieldRef::FoldSpecificField(f) => f.kind.named(),
        }
    }
}

impl NamedTypedValue for Argument {
    fn typed(&self) -> &Type {
        match self {
            Argument::Tag(t) => t.typed(),
            Argument::Variable(v) => v.typed(),
        }
    }

    fn named(&self) -> &str {
        match self {
            Argument::Tag(t) => t.named(),
            Argument::Variable(v) => v.named(),
        }
    }
}

pub(crate) fn are_base_types_equal_ignoring_nullability(left: &Type, right: &Type) -> bool {
    left.base_named_type() == right.base_named_type()
}

pub(crate) fn is_base_type_orderable(operand_type: &Type) -> bool {
    let name = operand_type.base_named_type();
    name == "Int" || name == "Float" || name == "String"
}

/// Check for scalar-only subtyping.
///
/// Scalars don't have an inheritance structure, so they are able to be compared without a schema.
/// Callers of this function must guarantee that the passed types are either scalars or
/// (potentially multiply-nested) lists of scalars.
///
/// This function considers types of different names to always be non-equal and unrelated:
/// neither is a subtype of the other. So given `interface Base` and `type Derived implements Base`,
/// that means `is_scalar_only_subtype(Base, Derived) == false`, since this function never sees
/// the definitions of `Base` and `Derived` as those are part of a schema which this function
/// never gets.
pub(crate) fn is_scalar_only_subtype(parent_type: &Type, maybe_subtype: &Type) -> bool {
    // If the parent type is non-nullable, all its subtypes must be non-nullable as well.
    // If the parent type is nullable, it can have both nullable and non-nullable subtypes.
    if !parent_type.is_nullable() && maybe_subtype.is_nullable() {
        return false;
    }

    match (parent_type.as_list(), maybe_subtype.as_list()) {
        (Some(parent), Some(maybe_subtype)) => is_scalar_only_subtype(&parent, &maybe_subtype),
        (None, None) => parent_type.base_named_type() == maybe_subtype.base_named_type(),
        _ => false,
    }
}

/// For two types, return a type that is a subtype of both, or None if no such type exists.
/// For example:
/// ```rust
/// use trustfall_core::ir::ty::Type;
/// use trustfall_core::ir::types::intersect_types;
///
/// let left = Type::new("[String]!").unwrap();
/// let right = Type::new("[String!]").unwrap();
/// let result = intersect_types(&left, &right);
/// assert_eq!(Some(Type::new("[String!]!").unwrap()), result);
///
/// let incompatible = Type::new("[Int]").unwrap();
/// let result = intersect_types(&left, &incompatible);
/// assert_eq!(None, result);
/// ```
pub fn intersect_types(left: &Type, right: &Type) -> Option<Type> {
    let nullable = left.is_nullable() && right.is_nullable();

    match (left.as_list(), right.as_list()) {
        (None, None) => {
            if left.base_named_type() == right.base_named_type() {
                Some(Type::new_named_type(left.base_named_type(), nullable))
            } else {
                None
            }
        }
        (Some(left), Some(right)) => {
            intersect_types(&left, &right).map(|inner| Type::new_list_type(inner, nullable))
        }
        _ => None,
    }
}

/// Check if the given argument value is valid for the specified variable type.
///
/// In particular, mixed integer types in a list are considered valid for types like `[Int]`.
/// ```rust
/// use trustfall_core::ir::ty::Type;
/// use trustfall_core::ir::{FieldValue, types::is_argument_type_valid};
///
/// let variable_type = Type::new("[Int]").unwrap();
/// let argument_value = FieldValue::List([
///     FieldValue::Int64(-1),
///     FieldValue::Uint64(1),
///     FieldValue::Null,
/// ].as_slice().into());
/// assert!(is_argument_type_valid(&variable_type, &argument_value));
/// ```
pub fn is_argument_type_valid(variable_type: &Type, argument_value: &FieldValue) -> bool {
    match argument_value {
        FieldValue::Null => {
            // This is a valid value only if this layer is nullable.
            variable_type.is_nullable()
        }
        FieldValue::Int64(_) | FieldValue::Uint64(_) => {
            // This is a valid value only if the type is Int, ignoring nullability.
            !variable_type.is_list() && variable_type.base_named_type() == "Int"
        }
        FieldValue::Float64(_) => {
            // This is a valid value only if the type is Float, ignoring nullability.
            !variable_type.is_list() && variable_type.base_named_type() == "Float"
        }
        FieldValue::String(_) => {
            // This is a valid value only if the type is String, ignoring nullability.
            !variable_type.is_list() && variable_type.base_named_type() == "String"
        }
        FieldValue::Boolean(_) => {
            // This is a valid value only if the type is Boolean, ignoring nullability.
            !variable_type.is_list() && variable_type.base_named_type() == "Boolean"
        }
        FieldValue::List(nested_values) => {
            // This is a valid value only if the type is a list, and all the inner elements
            // are valid instances of the type inside the list.
            if let Some(list) = variable_type.as_list() {
                nested_values.iter().all(|value| is_argument_type_valid(&list, value))
            } else {
                false
            }
        }
        FieldValue::Enum(_) => todo!(),
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use crate::ir::{ty::Type, types::is_argument_type_valid, FieldValue};

    #[test]
    fn null_values_are_only_valid_for_nullable_types() {
        let nullable_types = [
            Type::new("Int").unwrap(),
            Type::new("String").unwrap(),
            Type::new("Boolean").unwrap(),
            Type::new("[Int!]").unwrap(),
            Type::new("[[Int!]!]").unwrap(),
        ];
        let non_nullable_types =
            nullable_types.iter().map(|t| t.with_nullability(false)).collect_vec();

        for nullable_type in &nullable_types {
            assert!(is_argument_type_valid(nullable_type, &FieldValue::Null), "{}", nullable_type);
        }
        for non_nullable_type in &non_nullable_types {
            assert!(
                !is_argument_type_valid(non_nullable_type, &FieldValue::Null),
                "{}",
                non_nullable_type
            );
        }
    }

    #[test]
    fn int_values_are_valid_only_for_int_type_regardless_of_nullability() {
        let matching_types = [Type::new("Int").unwrap(), Type::new("Int!").unwrap()];
        let non_matching_types = [
            Type::new("String").unwrap(),
            Type::new("[Int!]").unwrap(),
            Type::new("[Int!]!").unwrap(),
            Type::new("[[Int!]!]").unwrap(),
        ];
        let values = [
            FieldValue::Int64(-42),
            FieldValue::Int64(0),
            FieldValue::Uint64(0),
            FieldValue::Uint64((i64::MAX as u64) + 1),
        ];

        for value in &values {
            for matching_type in &matching_types {
                assert!(is_argument_type_valid(matching_type, value), "{matching_type} {value:?}",);
            }
            for non_matching_type in &non_matching_types {
                assert!(
                    !is_argument_type_valid(non_matching_type, value),
                    "{non_matching_type} {value:?}",
                );
            }
        }
    }

    #[test]
    fn string_values_are_valid_only_for_string_type_regardless_of_nullability() {
        let matching_types = [Type::new("String").unwrap(), Type::new("String!").unwrap()];
        let non_matching_types = [
            Type::new("Int").unwrap(),
            Type::new("[String!]").unwrap(),
            Type::new("[String!]!").unwrap(),
            Type::new("[[String!]!]").unwrap(),
        ];
        let values = [
            FieldValue::String("".into()), // empty string is not the same value as null
            FieldValue::String("test string".into()),
        ];

        for value in &values {
            for matching_type in &matching_types {
                assert!(is_argument_type_valid(matching_type, value), "{matching_type} {value:?}",);
            }
            for non_matching_type in &non_matching_types {
                assert!(
                    !is_argument_type_valid(non_matching_type, value),
                    "{non_matching_type} {value:?}",
                );
            }
        }
    }

    #[test]
    fn boolean_values_are_valid_only_for_boolean_type_regardless_of_nullability() {
        let matching_types = [Type::new("Boolean").unwrap(), Type::new("Boolean!").unwrap()];
        let non_matching_types = [
            Type::new("Int").unwrap(),
            Type::new("[Boolean!]").unwrap(),
            Type::new("[Boolean!]!").unwrap(),
            Type::new("[[Boolean!]!]").unwrap(),
        ];
        let values = [FieldValue::Boolean(false), FieldValue::Boolean(true)];

        for value in &values {
            for matching_type in &matching_types {
                assert!(is_argument_type_valid(matching_type, value), "{matching_type} {value:?}",);
            }
            for non_matching_type in &non_matching_types {
                assert!(
                    !is_argument_type_valid(non_matching_type, value),
                    "{non_matching_type} {value:?}",
                );
            }
        }
    }

    #[test]
    fn list_types_correctly_check_contents_of_list() {
        let non_nullable_contents_matching_types =
            vec![Type::new("[Int!]").unwrap(), Type::new("[Int!]!").unwrap()];
        let nullable_contents_matching_types =
            vec![Type::new("[Int]").unwrap(), Type::new("[Int]!").unwrap()];
        let non_matching_types = [
            Type::new("Int").unwrap(),
            Type::new("Int!").unwrap(),
            Type::new("[String!]").unwrap(),
            Type::new("[String!]!").unwrap(),
            Type::new("[[String!]!]").unwrap(),
        ];
        let non_nullable_values = [
            FieldValue::List((1..3).map(FieldValue::Int64).collect_vec().into()),
            FieldValue::List((1..3).map(FieldValue::Uint64).collect_vec().into()),
            FieldValue::List(
                vec![
                    // Integer-typed but non-homogeneous FieldValue entries are okay.
                    FieldValue::Int64(-42),
                    FieldValue::Uint64(64),
                ]
                .into(),
            ),
        ];
        let nullable_values = [
            FieldValue::List(
                vec![FieldValue::Int64(1), FieldValue::Null, FieldValue::Int64(2)].into(),
            ),
            FieldValue::List(vec![FieldValue::Null, FieldValue::Uint64(42)].into()),
            FieldValue::List(
                vec![
                    // Integer-typed but non-homogeneous FieldValue entries are okay.
                    FieldValue::Int64(-1),
                    FieldValue::Uint64(1),
                    FieldValue::Null,
                ]
                .into(),
            ),
        ];

        for value in &non_nullable_values {
            // Values without nulls match both the nullable and the non-nullable types.
            for matching_type in &nullable_contents_matching_types {
                assert!(is_argument_type_valid(matching_type, value), "{matching_type} {value:?}",);
            }
            for matching_type in &non_nullable_contents_matching_types {
                assert!(is_argument_type_valid(matching_type, value), "{matching_type} {value:?}",);
            }

            // Regardless of nulls, these types don't match.
            for non_matching_type in &non_matching_types {
                assert!(
                    !is_argument_type_valid(non_matching_type, value),
                    "{non_matching_type} {value:?}",
                );
            }
        }

        for value in &nullable_values {
            // Nullable values match only the nullable types.
            for matching_type in &nullable_contents_matching_types {
                assert!(is_argument_type_valid(matching_type, value), "{matching_type} {value:?}",);
            }

            // The nullable values don't match the non-nullable types.
            for non_matching_type in &non_nullable_contents_matching_types {
                assert!(
                    !is_argument_type_valid(non_matching_type, value),
                    "{non_matching_type} {value:?}",
                );
            }

            // Regardless of nulls, these types don't match.
            for non_matching_type in &non_matching_types {
                assert!(
                    !is_argument_type_valid(non_matching_type, value),
                    "{non_matching_type} {value:?}",
                );
            }
        }
    }
}
