use std::fmt::Debug;

use super::{
    super::{
        Argument, ContextField, FieldRef, FoldSpecificField, FoldSpecificFieldKind, LocalField,
        OperationSubject, VariableRef,
    },
    Type,
};

pub trait NamedTypedValue: Debug + Clone + PartialEq + Eq {
    fn typed(&self) -> &Type;

    fn named(&self) -> &str;
}

impl NamedTypedValue for OperationSubject {
    fn typed(&self) -> &Type {
        match self {
            OperationSubject::LocalField(inner) => inner.typed(),
            OperationSubject::TransformedField(_) => todo!(),
        }
    }

    fn named(&self) -> &str {
        match self {
            OperationSubject::LocalField(inner) => inner.named(),
            OperationSubject::TransformedField(_) => todo!(),
        }
    }
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
