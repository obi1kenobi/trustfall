use std::{fmt::Debug, sync::Arc};

use crate::ir::{
    ContextField, FieldRef, FoldSpecificField, IRQueryComponent, LocalField, TransformBase, Vid,
};

use super::{
    execution::{
        compute_context_field_with_separate_value, compute_fold_specific_field_with_separate_value,
        compute_local_field_with_separate_value, QueryCarrier,
    },
    transformation::{
        apply_transforms, drop_unused_transform_arguments,
        push_transform_argument_tag_values_onto_stack_during_main_query,
    },
    Adapter, ContextIterator, DataContext, TaggedValue,
};

pub(super) fn compute_tag_with_separate_value<
    'query,
    AdapterT: Adapter<'query>,
    // For the contexts inside the input iterator, we sometimes may need to change what their
    // `ctx.active_vertex` value holds. This const generic controls whether we restore
    // the contents of `ctx.active_vertex` to its prior value after we're done, or not.
    const RESTORE_CONTEXT: bool,
>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    field_ref: &FieldRef,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> Box<dyn Iterator<Item = (DataContext<AdapterT::Vertex>, TaggedValue)> + 'query> {
    match field_ref {
        FieldRef::ContextField(context_field) => {
            compute_context_field_tag_with_separate_value::<AdapterT, RESTORE_CONTEXT>(
                current_vid,
                adapter,
                carrier,
                component,
                context_field,
                iterator,
            )
        }
        FieldRef::FoldSpecificField(fold_field) => {
            compute_fold_specific_field_tag_with_separate_value(component, fold_field, iterator)
        }
        FieldRef::TransformedField(transformed_field) => {
            let transform_arguments_iterator =
                push_transform_argument_tag_values_onto_stack_during_main_query(
                    adapter,
                    carrier,
                    component,
                    current_vid,
                    &transformed_field.value.transforms,
                    iterator,
                );

            let base_value_iterator = match &transformed_field.value.base {
                TransformBase::ContextField(context_field) => {
                    compute_context_field_tag_with_separate_value::<AdapterT, RESTORE_CONTEXT>(
                        current_vid,
                        adapter,
                        carrier,
                        component,
                        context_field,
                        transform_arguments_iterator,
                    )
                }
                TransformBase::FoldSpecificField(fold_field) => {
                    compute_fold_specific_field_tag_with_separate_value(
                        component,
                        fold_field,
                        transform_arguments_iterator,
                    )
                }
            };

            let transformed_value = Arc::clone(&transformed_field.value);

            let variables = carrier
                .query
                .as_ref()
                .map(|query| Arc::clone(&query.arguments))
                .expect("query was not returned");

            Box::new(base_value_iterator.map(move |(mut ctx, base_value)| {
                let value = match base_value {
                    TaggedValue::NonexistentOptional => {
                        // We may have pushed arguments onto the `ctx.values` stack for use by
                        // the transforms here, but it turns out we aren't going to need them.
                        // Remove them from the stack to avoid corrupting its state.
                        drop_unused_transform_arguments(&transformed_value, &mut ctx.values);

                        TaggedValue::NonexistentOptional
                    }
                    TaggedValue::Some(value) => TaggedValue::Some(apply_transforms(
                        &transformed_value,
                        &variables,
                        &mut ctx.values,
                        value,
                    )),
                };

                (ctx, value)
            }))
        }
    }
}

fn compute_context_field_tag_with_separate_value<
    'query,
    AdapterT: Adapter<'query>,
    // For the contexts inside the input iterator, we sometimes may need to change what their
    // `ctx.active_vertex` value holds. This const generic controls whether we restore
    // the contents of `ctx.active_vertex` to its prior value after we're done, or not.
    const RESTORE_CONTEXT: bool,
>(
    current_vid: Vid,
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    context_field: &ContextField,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> Box<dyn Iterator<Item = (DataContext<AdapterT::Vertex>, TaggedValue)> + 'query> {
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
        compute_context_field_with_separate_value::<AdapterT, AdapterT::Vertex, RESTORE_CONTEXT>(
            adapter,
            carrier,
            component,
            context_field,
            iterator,
        )
    }
}

fn compute_fold_specific_field_tag_with_separate_value<'query, Vertex: Debug + Clone + 'query>(
    component: &IRQueryComponent,
    fold_field: &FoldSpecificField,
    iterator: ContextIterator<'query, Vertex>,
) -> Box<dyn Iterator<Item = (DataContext<Vertex>, TaggedValue)> + 'query> {
    if component.folds.contains_key(&fold_field.fold_eid) {
        // This value represents a fold-specific field of a `@fold` that
        // is directly part of this query component, such as a `@fold @transform(op: "count")`.
        // That makes the fold-specific value directly computable via the folded `DataContext`s.
        compute_fold_specific_field_with_separate_value(
            fold_field.fold_eid,
            &fold_field.kind,
            iterator,
        )
    } else {
        // This value represents an imported tag value from an outer component.
        // Grab its value from the context itself.
        //
        // TODO: If we ever allow exporting tags from inside `@fold` (including getting
        //       fold-specific values of a *nested* fold), then those cases will end up
        //       inside this `else` block as well and we'll need to tell them apart.
        let moved_ref = FieldRef::FoldSpecificField(fold_field.to_owned());
        Box::new(iterator.map(move |ctx| {
            let right_value = ctx.imported_tags[&moved_ref].clone();
            (ctx, right_value)
        }))
    }
}
