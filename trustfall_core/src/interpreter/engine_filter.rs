//! Stream-native `@filter` handling for tag arguments.
//!
//! The left-hand field value has already been resolved and pushed onto each context's value stack
//! by `apply_local_field_filter`. This stage resolves the *tag's* value per context (an adapter
//! call, possibly on another vertex or an imported outer-component tag), then compares. Mirrors the
//! synchronous tag-filter path in [`filtering`](super::filtering) / [`execution`](super::execution)
//! (`compute_context_field_with_separate_value` + `apply_filter_with_tagged_argument_value`).
//!
//! The comparison itself is a [`ComparisonOp`], so tag filters apply exactly the same semantics
//! as runtime-argument filters — one source of truth for filter semantics.

use async_stream::try_stream;
use futures_util::StreamExt;

use crate::ir::{
    Argument, ContextField, Eid, FieldRef, FieldValue, FoldSpecificFieldKind, IRQueryComponent,
    LocalField, Operation, Vid,
};

use super::{
    TaggedValue,
    async_adapter::AsyncAdapter,
    engine::{FallibleContextStream, begin_stage, finish_stage},
    execution::QueryCarrier,
    filtering::ComparisonOp,
};

/// Apply a tag-argument `@filter`. The incoming `stream` already has the left field value pushed
/// onto each context's value stack.
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

/// Compare each context's already-pushed left value against a resolved tag value, keeping the
/// contexts that pass. Shared by local-field tag filters and post-`@fold` fold-count tag filters;
/// the left value must already sit on top of each context's value stack.
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
                // A fold-specific field (e.g. the fold count) from a `@fold` in this component. The
                // fold is already materialized by the time filters run, so the tag value is read
                // from `folded_contexts` — no adapter call needed.
                apply_fold_specific_tag_filter(op, fold_field.fold_eid, fold_field.kind, stream)
            } else {
                // Imported tag from an outer component stored in context.imported_tags.
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
    // Local tag: the field lives on the current vertex — resolve as a local property.
    if context_field.vertex_id == current_vid {
        let local_field = LocalField {
            field_name: context_field.field_name.clone(),
            field_type: context_field.field_type.clone(),
        };
        let type_name = component.vertices[&current_vid].type_name.clone();
        let (plain, upstream_error) = begin_stage(stream);
        let field_data = carrier.resolve_with(current_vid, true, |info| {
            adapter.resolve_property(plain, &type_name, &local_field.field_name, info)
        });

        let staged = finish_stage(field_data, upstream_error);
        return Box::pin(try_stream! {
            let mut staged = staged;
            while let Some(item) = staged.next().await {
                let (mut ctx, right_value) = item?;
                let left_value = ctx.values.pop().expect("no value present");
                // within_nonexistent_optional: filter vacuously passes (same as sync engine).
                if ctx.within_nonexistent_optional()
                    || passes_tagged_filter(op, &left_value, TaggedValue::Some(right_value))
                {
                    yield ctx;
                }
            }
        });
    }

    // Non-local context field within the current component: move to that vertex, resolve, restore.
    if let Some(vertex) = component.vertices.get(&context_field.vertex_id) {
        let vertex_id = context_field.vertex_id;
        let type_name = vertex.type_name.clone();
        let field_name = context_field.field_name.clone();

        let (plain, upstream_error) = begin_stage(stream);

        // Push current active vertex onto suspended_vertices and switch to the tag's vertex.
        let plain = Box::pin(plain.map(move |mut context| {
            let active_vertex = context.active_vertex.clone();
            let new_vertex = context.vertices[&vertex_id].clone();
            context.suspended_vertices.push(active_vertex);
            context.move_to_vertex(new_vertex)
        }));

        let field_data = carrier.resolve_with(vertex_id, true, |info| {
            adapter.resolve_property(plain, &type_name, &field_name, info)
        });

        let staged = finish_stage(field_data, upstream_error);
        return Box::pin(try_stream! {
            let mut staged = staged;
            while let Some(item) = staged.next().await {
                let (mut ctx, value) = item?;
                let tagged = if ctx.vertices[&vertex_id].is_some() {
                    TaggedValue::Some(value)
                } else {
                    TaggedValue::NonexistentOptional
                };
                // Restore the previous active vertex.
                let old_active = ctx.suspended_vertices.pop().unwrap();
                ctx = ctx.move_to_vertex(old_active);

                let left_value = ctx.values.pop().expect("no value present");
                // within_nonexistent_optional: filter vacuously passes (same as sync engine).
                if ctx.within_nonexistent_optional() || passes_tagged_filter(op, &left_value, tagged)
                {
                    yield ctx;
                }
            }
        });
    }

    // Imported outer-component tag: value is stored in context.imported_tags.
    let field_ref = FieldRef::ContextField(context_field);
    apply_imported_tag_filter(op, field_ref, stream)
}

fn apply_imported_tag_filter<'query, V: Clone + std::fmt::Debug + 'query, E: 'query>(
    op: ComparisonOp,
    field_ref: FieldRef,
    stream: FallibleContextStream<'query, V, E>,
) -> FallibleContextStream<'query, V, E> {
    Box::pin(try_stream! {
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            let mut ctx = item?;
            let tagged = ctx.imported_tags[&field_ref].clone();
            let left_value = ctx.values.pop().expect("no value present");
            // within_nonexistent_optional: filter vacuously passes.
            if ctx.within_nonexistent_optional() || passes_tagged_filter(op, &left_value, tagged) {
                yield ctx;
            }
        }
    })
}

/// Apply a filter whose tag refers to a fold-specific field (e.g. a fold count) of a `@fold` in the
/// current component. The value is derived from the already-materialized `folded_contexts`; a fold
/// inside a nonexistent `@optional` yields `NonexistentOptional` (the filter then passes).
fn apply_fold_specific_tag_filter<'query, V: Clone + std::fmt::Debug + 'query, E: 'query>(
    op: ComparisonOp,
    fold_eid: Eid,
    kind: FoldSpecificFieldKind,
    stream: FallibleContextStream<'query, V, E>,
) -> FallibleContextStream<'query, V, E> {
    Box::pin(try_stream! {
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            let mut ctx = item?;
            let tagged = match &kind {
                FoldSpecificFieldKind::Count => match ctx.folded_contexts[&fold_eid].as_ref() {
                    None => TaggedValue::NonexistentOptional,
                    Some(elements) => TaggedValue::Some(FieldValue::Uint64(elements.len() as u64)),
                },
            };
            let left_value = ctx.values.pop().expect("no value present");
            if ctx.within_nonexistent_optional() || passes_tagged_filter(op, &left_value, tagged) {
                yield ctx;
            }
        }
    })
}

/// Return true if the context should be kept given the filter operation and tagged right-hand value.
///
/// `NonexistentOptional` always passes (the filter is vacuous against an absent @optional scope).
fn passes_tagged_filter(op: ComparisonOp, left: &FieldValue, tagged: TaggedValue) -> bool {
    let TaggedValue::Some(right) = tagged else {
        // NonexistentOptional: always pass.
        return true;
    };
    op.apply(left, &right)
}
