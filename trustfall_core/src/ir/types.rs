use async_graphql_parser::types::{BaseType, Type};

pub(crate) fn are_base_types_equal_ignoring_nullability(left: &BaseType, right: &BaseType) -> bool {
    match (left, right) {
        (BaseType::Named(l), BaseType::Named(r)) => l == r,
        (BaseType::List(l), BaseType::List(r)) => {
            are_base_types_equal_ignoring_nullability(&l.base, &r.base)
        }
        (BaseType::Named(_), BaseType::List(_)) | (BaseType::List(_), BaseType::Named(_)) => false,
    }
}

pub(crate) fn is_base_type_orderable(operand_type: &BaseType) -> bool {
    match operand_type {
        BaseType::Named(name) => {
            name == "Int" || name == "Float" || name == "String" || name == "DateTime"
        }
        BaseType::List(l) => is_base_type_orderable(&l.base),
    }
}

pub(crate) fn is_subtype(parent_type: &Type, maybe_subtype: &Type) -> bool {
    // If the parent type is nullable, all its subtypes are be nullable as well.
    // If the parent type is nullable, it can have both nullable and non-nullable subtypes.
    if parent_type.nullable && !maybe_subtype.nullable {
        return false;
    }

    match (&parent_type.base, &maybe_subtype.base) {
        (BaseType::Named(parent), BaseType::Named(subtype)) => parent == subtype,
        (BaseType::List(parent_type), BaseType::List(maybe_subtype)) => {
            is_subtype(parent_type, maybe_subtype)
        }
        (BaseType::Named(..), BaseType::List(..)) | (BaseType::List(..), BaseType::Named(..)) => {
            false
        }
    }
}

/// For two types, return a type that is a subtype of both, or None if no such type exists.
/// For example:
/// ```rust
/// use async_graphql_parser::types::Type;
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
    let nullable = left.nullable && right.nullable;

    match (&left.base, &right.base) {
        (BaseType::Named(l), BaseType::Named(r)) => {
            if l == r {
                Some(Type {
                    base: left.base.clone(),
                    nullable,
                })
            } else {
                None
            }
        }
        (BaseType::List(left), BaseType::List(right)) => {
            intersect_types(left, right).map(|inner| Type {
                base: BaseType::List(Box::new(inner)),
                nullable,
            })
        }
        (BaseType::Named(_), BaseType::List(_)) | (BaseType::List(_), BaseType::Named(_)) => None,
    }
}
