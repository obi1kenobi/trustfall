use std::fmt::Debug;

use super::{
    super::{
        Argument, ContextField, FieldRef, FoldSpecificField, FoldSpecificFieldKind, LocalField,
        VariableRef,
    },
    Type,
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

pub(crate) fn is_base_type_orderable(operand_type: &Type) -> bool {
    let name = operand_type.base_type();
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
    if !parent_type.nullable() && maybe_subtype.nullable() {
        return false;
    }

    // If the base types don't match, there can't be a subtyping relationship here.
    // Recall that callers are required to make sure only scalar / nested-lists-of-scalar types
    // are passed into this function.
    if parent_type.base_type() != maybe_subtype.base_type() {
        return false;
    }

    match (parent_type.as_list(), maybe_subtype.as_list()) {
        (None, None) => true,
        (Some(parent), Some(maybe_subtype)) => is_scalar_only_subtype(&parent, &maybe_subtype),
        _ => false,
    }
}
