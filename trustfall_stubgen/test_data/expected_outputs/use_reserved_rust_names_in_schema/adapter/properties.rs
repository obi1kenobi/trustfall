use trustfall::{FieldValue, provider::{ContextIterator, ContextOutcomeIterator, ResolveInfo}};

use super::vertex::Vertex;

pub(super) fn resolve_type2_property<'a>(
    contexts: ContextIterator<'a, Vertex>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, Vertex, FieldValue> {
    match property_name {
        "type" => todo!("implement property 'type' in fn `resolve_type2_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'Type2'"
            )
        }
    }
}
