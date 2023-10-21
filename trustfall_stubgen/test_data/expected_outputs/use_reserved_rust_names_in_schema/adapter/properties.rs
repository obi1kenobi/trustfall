use trustfall::{FieldValue, provider::{AsVertex, ContextIterator, ContextOutcomeIterator, ResolveInfo}};

use super::vertex::Vertex;

pub(super) fn resolve_const2_property<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, V, FieldValue> {
    match property_name {
        "const" => todo!("implement property 'const' in fn `resolve_const2_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'const2'"
            )
        }
    }
}

pub(super) fn resolve_continue2_property<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, V, FieldValue> {
    match property_name {
        "continue" => {
            todo!("implement property 'continue' in fn `resolve_continue2_property()`")
        }
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'continue2'"
            )
        }
    }
}

pub(super) fn resolve_dyn2_property<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, V, FieldValue> {
    match property_name {
        "dyn" => todo!("implement property 'dyn' in fn `resolve_dyn2_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'dyn2'"
            )
        }
    }
}

pub(super) fn resolve_if2_property<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, V, FieldValue> {
    match property_name {
        "if" => todo!("implement property 'if' in fn `resolve_if2_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'if2'"
            )
        }
    }
}

pub(super) fn resolve_mod2_property<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, V, FieldValue> {
    match property_name {
        "mod" => todo!("implement property 'mod' in fn `resolve_mod2_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'mod2'"
            )
        }
    }
}

pub(super) fn resolve_self2_property<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, V, FieldValue> {
    match property_name {
        "self" => todo!("implement property 'self' in fn `resolve_self2_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'self2'"
            )
        }
    }
}

pub(super) fn resolve_type2_property<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, V, FieldValue> {
    match property_name {
        "type" => todo!("implement property 'type' in fn `resolve_type2_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'type2'"
            )
        }
    }
}

pub(super) fn resolve_unsafe2_property<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, V, FieldValue> {
    match property_name {
        "unsafe" => {
            todo!("implement property 'unsafe' in fn `resolve_unsafe2_property()`")
        }
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'unsafe2'"
            )
        }
    }
}

pub(super) fn resolve_where2_property<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, V, FieldValue> {
    match property_name {
        "where" => todo!("implement property 'where' in fn `resolve_where2_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'where2'"
            )
        }
    }
}
