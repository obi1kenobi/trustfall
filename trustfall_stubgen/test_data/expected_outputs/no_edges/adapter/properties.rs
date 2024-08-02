use trustfall::{FieldValue, provider::{AsVertex, ContextIterator, ContextOutcomeIterator, ResolveInfo}};

use super::vertex::Vertex;

pub(super) fn resolve_item_property<'a, V: AsVertex<Vertex> + 'a>(
    contexts: ContextIterator<'a, V>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, V, FieldValue> {
    match property_name {
        "id" => todo!("implement property 'id' in fn `resolve_item_property()`"),
        "unixTime" => {
            todo!("implement property 'unixTime' in fn `resolve_item_property()`")
        }
        "url" => todo!("implement property 'url' in fn `resolve_item_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'Item'"
            )
        }
    }
}
