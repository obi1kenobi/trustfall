use crate::ir::{FieldRef, IRQueryComponent, LocalField, Vid};

use super::{
    execution::{
        compute_context_field_with_separate_value, compute_fold_specific_field_with_separate_value,
        compute_local_field_with_separate_value, QueryCarrier,
    },
    Adapter, ContextIterator, DataContext, TaggedValue,
};

pub(super) fn compute_tag_with_separate_value<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    field_ref: &FieldRef,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> Box<dyn Iterator<Item = (DataContext<AdapterT::Vertex>, TaggedValue)> + 'query> {
    match field_ref {
        FieldRef::ContextField(context_field) => {
            // TODO: Benchmark if it would be faster to duplicate the code to special-case
            //       the situation when the tag is always known to exist, so we don't have to unwrap
            //       a TaggedValue enum, because we know it would be TaggedValue::Some.
            if context_field.vertex_id == current_vid {
                // This tag is from the vertex we're currently evaluating. That means the field
                // whose value we want to get is actually local, so there's no need to compute it
                // using the more expensive approach we use for non-local fields.
                let local_equivalent_field = LocalField {
                    field_name: context_field.field_name.clone(),
                    field_type: context_field.field_type.clone(),
                };
                Box::new(
                    compute_local_field_with_separate_value(
                        adapter,
                        carrier,
                        component,
                        current_vid,
                        &local_equivalent_field,
                        iterator,
                    )
                    .map(|(ctx, value)| (ctx, TaggedValue::Some(value))),
                )
            } else {
                compute_context_field_with_separate_value(
                    adapter,
                    carrier,
                    component,
                    context_field,
                    iterator,
                )
            }
        }
        FieldRef::FoldSpecificField(fold_field) => {
            if component.folds.contains_key(&fold_field.fold_eid) {
                compute_fold_specific_field_with_separate_value(
                    fold_field.fold_eid,
                    &fold_field.kind,
                    iterator,
                )
            } else {
                // This value represents an imported tag value from an outer component.
                // Grab its value from the context itself.
                let cloned_ref = field_ref.clone();
                Box::new(iterator.map(move |ctx| {
                    let right_value = ctx.imported_tags[&cloned_ref].clone();
                    (ctx, right_value)
                }))
            }
        }
    }
}
