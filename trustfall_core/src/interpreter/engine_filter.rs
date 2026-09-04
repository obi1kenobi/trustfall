//! Stream handling for `@filter` directives that use tags.
//!
//! The local field has already been resolved and pushed onto `DataContext::values`. This module
//! finds the tagged value, compares it with that left value, and removes the stack entry before
//! forwarding the context. The tag can come from the active vertex, another local vertex, a fold,
//! or an enclosing component; those locations determine whether resolving it needs an adapter call.

use std::future::ready;

use futures_util::StreamExt;

use crate::ir::{
    Argument, ContextField, Eid, FieldRef, FieldValue, FoldSpecificFieldKind, IRQueryComponent,
    LocalField, Operation, Vid,
};

use super::{
    DataContext, TaggedValue,
    async_adapter::{AsyncAdapter, ContextOutcomeStream},
    engine::FallibleContextStream,
    execution::QueryCarrier,
    filtering::ComparisonOp,
};

/// Apply a tag-argument `@filter` to a stream with its left value resolved.
///
/// The caller has resolved the filter's local field. We retain that ordering because a tagged
/// field may itself need resolution and must not overwrite the left value it is compared against.
pub(super) fn apply_tagged_filter<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    filter: &Operation<LocalField, Argument>,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let tag_ref = match filter.right() {
        Some(Argument::Tag(field_ref)) => field_ref.clone(),
        _ => unreachable!("apply_tagged_filter called on non-tag filter: {filter:?}"),
    };

    let op = ComparisonOp::from_binary_filter(&filter.map(|_| (), |r| r))
        .expect("tag filters are binary operations");

    apply_tag_comparison(adapter, carrier, component, current_vid, op, tag_ref, stream)
}

/// Compare the resolved tag value with each context's left value.
///
/// Local context fields may be resolved from the adapter. Fields from an outer component were
/// copied into `DataContext::imported_tags` when entering the nested component. Fold-specific
/// fields are already materialized by the time a post-fold filter can read them.
pub(super) fn apply_tag_comparison<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    op: ComparisonOp,
    tag_ref: FieldRef,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    match tag_ref {
        FieldRef::ContextField(context_field) => apply_context_field_tagged_filter(
            adapter,
            carrier,
            component,
            current_vid,
            op,
            context_field,
            stream,
        ),
        FieldRef::FoldSpecificField(fold_field) => {
            if component.folds.contains_key(&fold_field.fold_eid) {
                apply_fold_specific_tag_filter(op, fold_field.fold_eid, fold_field.kind, stream)
            } else {
                let field_ref = FieldRef::FoldSpecificField(fold_field);
                apply_imported_tag_filter(op, field_ref, stream)
            }
        }
    }
}

fn apply_context_field_tagged_filter<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    op: ComparisonOp,
    context_field: ContextField,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    if context_field.vertex_id == current_vid {
        // The tag belongs to the active vertex. Resolve it directly, leaving the filter's left
        // value below it on the context stack until `tagged_filter_matches()` consumes it.
        let local_field = LocalField {
            field_name: context_field.field_name.clone(),
            field_type: context_field.field_type.clone(),
        };
        let type_name = component.vertices[&current_vid].type_name.clone();
        let field_data = carrier.resolve_with(current_vid, true, |info| {
            adapter.resolve_property(stream, &type_name, &local_field.field_name, info)
        });

        return filter_resolved_contexts(field_data, move |mut ctx, right_value| {
            tagged_filter_matches(&mut ctx, op, TaggedValue::Some(right_value)).then_some(ctx)
        });
    }

    if let Some(vertex) = component.vertices.get(&context_field.vertex_id) {
        // The tag belongs to another vertex in this component. Resolver helpers always inspect
        // the active vertex, so save the current one, activate the tagged vertex, then restore it
        // before evaluating the comparison.
        let vertex_id = context_field.vertex_id;
        let type_name = vertex.type_name.clone();
        let field_name = context_field.field_name.clone();

        let stream = Box::pin(stream.map(move |result| {
            result.map(|mut context| {
                let active_vertex = context.active_vertex.clone();
                let new_vertex = context.vertices[&vertex_id].clone();
                context.suspended_vertices.push(active_vertex);
                context.move_to_vertex(new_vertex)
            })
        }));

        let field_data = carrier.resolve_with(vertex_id, true, |info| {
            adapter.resolve_property(stream, &type_name, &field_name, info)
        });

        return filter_resolved_contexts(field_data, move |mut ctx, value| {
            let tagged = if ctx.vertices[&vertex_id].is_some() {
                TaggedValue::Some(value)
            } else {
                TaggedValue::NonexistentOptional
            };
            // This balances the temporary activation above. The filter continues at the
            // original vertex, which is where later fields and edges expect the context.
            let old_active = ctx.suspended_vertices.pop().unwrap();
            ctx = ctx.move_to_vertex(old_active);

            tagged_filter_matches(&mut ctx, op, tagged).then_some(ctx)
        });
    }

    // The component does not own this vertex, so the tag was imported before the component began.
    let field_ref = FieldRef::ContextField(context_field);
    apply_imported_tag_filter(op, field_ref, stream)
}

fn apply_imported_tag_filter<'query, V: 'query, E: 'query>(
    op: ComparisonOp,
    field_ref: FieldRef,
    stream: FallibleContextStream<'query, V, E>,
) -> FallibleContextStream<'query, V, E> {
    retain_contexts(stream, move |ctx| {
        let tagged = ctx.imported_tags[&field_ref].clone();
        tagged_filter_matches(ctx, op, tagged)
    })
}

/// Retain successful contexts that satisfy `predicate`, while preserving failed row positions.
fn retain_contexts<'query, V: 'query, E: 'query>(
    stream: FallibleContextStream<'query, V, E>,
    mut predicate: impl FnMut(&mut DataContext<V>) -> bool + 'query,
) -> FallibleContextStream<'query, V, E> {
    Box::pin(stream.filter_map(move |item| {
        ready(match item {
            Ok(mut context) => predicate(&mut context).then_some(Ok(context)),
            Err(error) => Some(Err(error)),
        })
    }))
}

/// Apply a predicate to resolved values, retaining the context only on success.
///
/// Resolver errors remain items in the output stream, so the stage cannot accidentally turn a
/// row-local failure into a dropped row.
fn filter_resolved_contexts<'query, V: 'query, T: 'query, E: 'query>(
    outcomes: ContextOutcomeStream<'query, V, T, E>,
    mut filter: impl FnMut(DataContext<V>, T) -> Option<DataContext<V>> + 'query,
) -> FallibleContextStream<'query, V, E> {
    Box::pin(outcomes.filter_map(move |item| {
        ready(match item {
            Ok((context, value)) => filter(context, value).map(Ok),
            Err(error) => Some(Err(error)),
        })
    }))
}

/// Apply a filter tagged with a field from an already-materialized fold.
///
/// A fold inside a nonexistent optional is represented by `None`, which is the same
/// `NonexistentOptional` tag value used for a missing tagged context field.
fn apply_fold_specific_tag_filter<'query, V: 'query, E: 'query>(
    op: ComparisonOp,
    fold_eid: Eid,
    kind: FoldSpecificFieldKind,
    stream: FallibleContextStream<'query, V, E>,
) -> FallibleContextStream<'query, V, E> {
    retain_contexts(stream, move |ctx| {
        let tagged = match &kind {
            FoldSpecificFieldKind::Count => match ctx.folded_contexts[&fold_eid].as_ref() {
                None => TaggedValue::NonexistentOptional,
                Some(elements) => TaggedValue::Some(FieldValue::Uint64(elements.len() as u64)),
            },
        };
        tagged_filter_matches(ctx, op, tagged)
    })
}

/// Remove this filter's left value and compare it with a tagged value.
///
/// Missing optional data passes every comparison. The optional scope represents absence, not a
/// value that can fail a predicate, so an inner filter must not discard the parent context.
fn tagged_filter_matches<Vertex>(
    context: &mut super::DataContext<Vertex>,
    op: ComparisonOp,
    tagged: TaggedValue,
) -> bool {
    let left = context.values.pop().expect("no value present");
    context.within_nonexistent_optional() || passes_tagged_filter(op, &left, tagged)
}

fn passes_tagged_filter(op: ComparisonOp, left: &FieldValue, tagged: TaggedValue) -> bool {
    let TaggedValue::Some(right) = tagged else {
        // A tagged value can also be absent because it came from an optional ancestor component.
        // Preserve the context for the same reason as `within_nonexistent_optional()` above.
        return true;
    };
    op.apply(left, &right)
}
