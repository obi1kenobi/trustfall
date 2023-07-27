use trustfall::{FieldValue, provider::{ContextIterator, ContextOutcomeIterator, ResolveInfo}};

use super::vertex::Vertex;

pub(super) fn resolve_unsafe2_property<'a>(
    contexts: ContextIterator<'a, Vertex>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, Vertex, FieldValue> {
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

pub(super) fn resolve_use2_property<'a>(
    contexts: ContextIterator<'a, Vertex>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, Vertex, FieldValue> {
    match property_name {
        "use" => todo!("implement property 'use' in fn `resolve_use2_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'use2'"
            )
        }
    }
}
