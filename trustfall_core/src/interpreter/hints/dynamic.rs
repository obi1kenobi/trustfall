use std::{fmt::Debug, ops::Bound, sync::Arc};

use crate::{
    interpreter::{
        Adapter, AsVertex, ContextIterator, ContextOutcomeIterator, InterpretedQuery, ResolveInfo,
        TaggedValue, VertexIterator, hints::Range,
    },
    ir::{
        ContextField, FieldRef, FieldValue, FoldSpecificField, IRQueryComponent, Operation, Type,
    },
};

use super::CandidateValue;

/// Indicates that a property's value is dependent on another value in the query.
///
/// If [`VertexInfo::dynamically_required_property()`](super::VertexInfo::dynamically_required_property)
/// is able to determine a value for the specified property, it returns
/// a [`DynamicallyResolvedValue`]. The specified property's value may be different
/// in different query results, but the way in which it varies can be determined programmatically
/// and can be resolved to a [`CandidateValue`] for each query result.
///
/// # Example
///
/// Consider the following query, which fetches emails where the sender also included
/// their own address in the receipients:
/// ```graphql
/// {
///     Email {
///         contents @output
///
///         sender {
///             address @tag(name: "sender")
///         }
///         recipient {
///             address @filter(op: "=", value: ["%sender"])
///         }
///     }
/// }
/// ```
///
/// A naïve implementation of resolving the `recipient` edge would resolve all recipients
/// for each email and rely on Trustfall to filter out recipient addresses that don't match
/// the sender's address. This implementation is valid, but can be made faster.
///
/// To improve performance, the implementation could avoid loading _all_ recipients and instead
/// only load the recipient that matches the sender's address (if any).
///
/// However, as the sender's address varies from email to email, its value must be resolved
/// dynamically, i.e. separately for each possible query result. Resolving the `recipient` edge
/// might then look like this:
/// ```rust
/// # use std::sync::Arc;
/// # use trustfall_core::{
/// #     ir::{EdgeParameters, FieldValue},
/// #     interpreter::{
/// #         Adapter, AsVertex, CandidateValue, ContextIterator, ContextOutcomeIterator,
/// #         ResolveEdgeInfo, ResolveInfo, VertexInfo, VertexIterator,
/// #     },
/// # };
/// # #[derive(Debug, Clone)]
/// # struct Vertex;
/// # struct EmailAdapter;
/// # impl<'a> Adapter<'a> for EmailAdapter {
/// #     type Vertex = Vertex;
/// #
/// #     type Error = std::convert::Infallible;
/// #
/// #     fn resolve_starting_vertices(
/// #         &self,
/// #         edge_name: &Arc<str>,
/// #         parameters: &EdgeParameters,
/// #         resolve_info: &ResolveInfo,
/// #     ) -> VertexIterator<'a, Result<Self::Vertex, Self::Error>> {
/// #         todo!()
/// #     }
/// #
/// #     fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
/// #         &self,
/// #         contexts: ContextIterator<'a, V>,
/// #         type_name: &Arc<str>,
/// #         property_name: &Arc<str>,
/// #         resolve_info: &ResolveInfo,
/// #     ) -> ContextOutcomeIterator<'a, V, Result<FieldValue, Self::Error>> {
/// #         todo!()
/// #     }
/// #
/// #     fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
/// #         &self,
/// #         contexts: ContextIterator<'a, V>,
/// #         type_name: &Arc<str>,
/// #         edge_name: &Arc<str>,
/// #         parameters: &EdgeParameters,
/// #         resolve_info: &ResolveEdgeInfo,
/// #     ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Result<Self::Vertex, Self::Error>>> {
/// #         todo!()
/// #     }
/// #
/// #     fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
/// #         &self,
/// #         contexts: ContextIterator<'a, V>,
/// #         type_name: &Arc<str>,
/// #         coerce_to_type: &Arc<str>,
/// #         resolve_info: &ResolveInfo,
/// #     ) -> ContextOutcomeIterator<'a, V, Result<bool, Self::Error>> {
/// #         todo!()
/// #     }
/// # }
/// #
/// # fn resolve_recipient_from_candidate_value<'a, V>(
/// #     vertex: &V,
/// #     candidate: CandidateValue<FieldValue>
/// # ) -> VertexIterator<'a, Vertex> {
/// #     todo!()
/// # }
/// #
/// # fn resolve_recipient_otherwise<'a, V>(
/// #     contexts: ContextIterator<'a, V>,
/// # ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Result<Vertex, std::convert::Infallible>>> {
/// #     todo!()
/// # }
/// #
/// # impl EmailAdapter {
/// // Inside our adapter implementation:
/// // we use this method to resolve `recipient` edges.
/// fn resolve_recipient_edge<'a, V: AsVertex<Vertex> + 'a>(
///     &self,
///     contexts: ContextIterator<'a, V>,
///     resolve_info: &ResolveEdgeInfo,
/// ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Result<Vertex, std::convert::Infallible>>> {
///     if let Some(dynamic_value) = resolve_info.destination().dynamically_required_property("address") {
///         // The query is looking for a specific recipient's address,
///         // so let's look it up directly.
///         dynamic_value.resolve_with(self, contexts, |vertex, candidate| {
///             resolve_recipient_from_candidate_value(vertex, candidate)
///         })
///     } else {
///         // No specific recipient address, use the general-case edge resolver logic.
///         resolve_recipient_otherwise(contexts)
///     }
/// }
/// # }
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicallyResolvedValue<'a> {
    query: InterpretedQuery,
    resolve_on_component: &'a IRQueryComponent,
    field: &'a FieldRef,
    operation: Operation<(), ()>,
    initial_candidate: CandidateValue<FieldValue>,
}

impl<'a> DynamicallyResolvedValue<'a> {
    pub(super) fn new(
        query: InterpretedQuery,
        resolve_on_component: &'a IRQueryComponent,
        field: &'a FieldRef,
        operation: Operation<(), ()>,
        initial_candidate: CandidateValue<FieldValue>,
    ) -> Self {
        Self { query, resolve_on_component, field, operation, initial_candidate }
    }

    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    pub fn resolve<'vertex, AdapterT: Adapter<'vertex>, V: AsVertex<AdapterT::Vertex> + 'vertex>(
        self,
        adapter: &AdapterT,
        contexts: ContextIterator<'vertex, V>,
    ) -> ContextOutcomeIterator<'vertex, V, Result<CandidateValue<FieldValue>, AdapterT::Error>>
    {
        // Only the `compute_candidate_from_tagged_value` branch touches the (fallible) adapter,
        // so it surfaces `Result` outcomes directly; the tag-from-context branches are infallible
        // and are wrapped in `Ok` to unify the outcome type.
        match &self.field {
            FieldRef::ContextField(context_field) => {
                if context_field.vertex_id < self.resolve_on_component.root {
                    // We're inside at least one level of `@fold` relative to
                    // the origin of this tag.
                    //
                    // We'll have to grab the tag's value from the context directly.
                    let field_ref = self.field;
                    ok_outcomes(self.compute_candidate_from_tagged_value_with_imported_tags(
                        field_ref, contexts,
                    ))
                } else {
                    self.compute_candidate_from_tagged_value(context_field, adapter, contexts)
                }
            }
            FieldRef::FoldSpecificField(fold_field) => {
                // TODO cover this with tests
                if fold_field.fold_root_vid < self.resolve_on_component.root {
                    // We're inside at least one level of `@fold` relative to
                    // the origin of this tag.
                    //
                    // We'll have to grab the tag's value from the context directly.
                    let field_ref = self.field;
                    ok_outcomes(self.compute_candidate_from_tagged_value_with_imported_tags(
                        field_ref, contexts,
                    ))
                } else {
                    ok_outcomes(self.resolve_fold_specific_field(fold_field, contexts))
                }
            }
        }
    }

    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    #[allow(clippy::type_complexity)]
    pub fn resolve_with<
        'vertex,
        AdapterT: Adapter<'vertex>,
        V: AsVertex<AdapterT::Vertex> + 'vertex,
    >(
        self,
        adapter: &AdapterT,
        contexts: ContextIterator<'vertex, V>,
        mut neighbor_resolver: impl FnMut(
            &AdapterT::Vertex,
            CandidateValue<FieldValue>,
        ) -> VertexIterator<'vertex, AdapterT::Vertex>
        + 'vertex,
    ) -> ContextOutcomeIterator<
        'vertex,
        V,
        VertexIterator<'vertex, Result<AdapterT::Vertex, AdapterT::Error>>,
    > {
        Box::new(self.resolve(adapter, contexts).map(move |(ctx, candidate)| {
            let neighbors: VertexIterator<'vertex, Result<AdapterT::Vertex, AdapterT::Error>> =
                match candidate {
                    // Surface a dynamic-resolution error into the neighbor stream so the
                    // engine's outer error tracking sees it and fails the query fast.
                    Err(error) => Box::new(std::iter::once(Err(error))),
                    Ok(candidate) => {
                        match ctx.active_vertex.as_ref().and_then(AsVertex::as_vertex) {
                            Some(vertex) => Box::new(neighbor_resolver(vertex, candidate).map(Ok)),
                            None => Box::new(std::iter::empty()),
                        }
                    }
                };
            (ctx, neighbors)
        }))
    }

    fn compute_candidate_from_tagged_value<
        'vertex,
        AdapterT: Adapter<'vertex>,
        V: AsVertex<AdapterT::Vertex> + 'vertex,
    >(
        self,
        context_field: &'a ContextField,
        adapter: &AdapterT,
        contexts: ContextIterator<'vertex, V>,
    ) -> ContextOutcomeIterator<'vertex, V, Result<CandidateValue<FieldValue>, AdapterT::Error>>
    {
        let vertex_id = context_field.vertex_id;
        let field_name = context_field.field_name.clone();
        let field_type = context_field.field_type.clone();
        let operation = self.operation;
        let initial_candidate = self.initial_candidate;

        let Some(vertex) = self.resolve_on_component.vertices.get(&vertex_id) else {
            let field_ref = FieldRef::ContextField(context_field.clone());
            return Box::new(contexts.map(move |context| {
                let tagged = context.imported_tags[&field_ref].clone();
                let candidate = candidate_from_tagged_value(
                    &operation,
                    &initial_candidate,
                    &field_name,
                    &field_type,
                    true,
                    tagged,
                );
                (context, Ok(candidate))
            }));
        };

        let contexts = contexts.map(move |mut context| {
            let active_vertex = context.active_vertex.clone();
            let tagged_vertex = context.vertices[&vertex_id].clone();
            context.suspended_vertices.push(active_vertex);
            context.move_to_vertex(tagged_vertex)
        });

        let resolve_info = ResolveInfo::new(self.query, vertex_id, true);
        let outcomes = adapter.resolve_property(
            Box::new(contexts),
            &vertex.type_name,
            &field_name,
            &resolve_info,
        );

        Box::new(outcomes.map(move |(mut context, outcome)| {
            let tagged = outcome.map(|value| {
                if context.vertices[&vertex_id].is_some() {
                    TaggedValue::Some(value)
                } else {
                    TaggedValue::NonexistentOptional
                }
            });
            let previous_vertex = context.suspended_vertices.pop().unwrap();
            let context = context.move_to_vertex(previous_vertex);
            let candidate = tagged.map(|tagged| {
                candidate_from_tagged_value(
                    &operation,
                    &initial_candidate,
                    &field_name,
                    &field_type,
                    true,
                    tagged,
                )
            });
            (context, candidate)
        }))
    }

    fn compute_candidate_from_tagged_value_with_imported_tags<
        'vertex,
        VertexT: Debug + Clone + 'vertex,
    >(
        self,
        field_ref: &'a FieldRef,
        contexts: ContextIterator<'vertex, VertexT>,
    ) -> ContextOutcomeIterator<'vertex, VertexT, CandidateValue<FieldValue>> {
        let cloned_field_ref = field_ref.clone();
        let iterator = Box::new(contexts.map(move |ctx| {
            let value = ctx.imported_tags[&cloned_field_ref].clone();
            (ctx, value)
        }));
        let (field_name, field_type) = match field_ref {
            FieldRef::ContextField(c) => (c.field_name.clone(), c.field_type.clone()),
            FieldRef::FoldSpecificField(f) => {
                (f.kind.field_name().into(), f.kind.field_type().clone())
            }
        };
        compute_candidate_from_operation(
            &self.operation,
            self.initial_candidate,
            field_name,
            field_type,
            iterator,
        )
    }

    fn resolve_fold_specific_field<'vertex, VertexT: Debug + Clone + 'vertex>(
        self,
        fold_field: &'a FoldSpecificField,
        contexts: ContextIterator<'vertex, VertexT>,
    ) -> ContextOutcomeIterator<'vertex, VertexT, CandidateValue<FieldValue>> {
        let fold_eid = fold_field.fold_eid;
        let iterator = contexts.map(move |context| {
            let tagged = match context.folded_contexts[&fold_eid].as_ref() {
                None => TaggedValue::NonexistentOptional,
                Some(values) => TaggedValue::Some(FieldValue::Uint64(values.len() as u64)),
            };
            (context, tagged)
        });
        let initial_candidate = self.initial_candidate;
        let operation = self.operation;
        let field_name: Arc<str> = fold_field.kind.field_name().into();
        let field_type = fold_field.kind.field_type().clone();
        Box::new(iterator.map(move |(context, tagged)| {
            let candidate = candidate_from_tagged_value(
                &operation,
                &initial_candidate,
                &field_name,
                &field_type,
                false,
                tagged,
            );
            (context, candidate)
        }))
    }
}

/// The single boundary where the infallible tag-from-context resolution paths meet
/// [`DynamicallyResolvedValue::resolve`]'s fallible outcome type: each candidate is wrapped in
/// `Ok`. Keeping this here means the infallible helpers (and the `compute_candidate_*` macros)
/// never mention `Result` themselves.
fn ok_outcomes<'vertex, V: 'vertex, E: 'vertex>(
    outcomes: ContextOutcomeIterator<'vertex, V, CandidateValue<FieldValue>>,
) -> ContextOutcomeIterator<'vertex, V, Result<CandidateValue<FieldValue>, E>> {
    Box::new(outcomes.map(|(ctx, candidate)| (ctx, Ok(candidate))))
}

fn compute_candidate_from_operation<'vertex, Vertex: Debug + Clone + 'vertex>(
    operation: &Operation<(), ()>,
    initial_candidate: CandidateValue<FieldValue>,
    field_name: Arc<str>,
    field_type: Type,
    iterator: ContextOutcomeIterator<'vertex, Vertex, TaggedValue>,
) -> ContextOutcomeIterator<'vertex, Vertex, CandidateValue<FieldValue>> {
    let operation = operation.clone();
    Box::new(iterator.map(move |(context, tagged)| {
        let candidate = candidate_from_tagged_value(
            &operation,
            &initial_candidate,
            &field_name,
            &field_type,
            true,
            tagged,
        );
        (context, candidate)
    }))
}

fn candidate_from_tagged_value(
    operation: &Operation<(), ()>,
    initial: &CandidateValue<FieldValue>,
    field_name: &Arc<str>,
    field_type: &Type,
    nullable: bool,
    tagged: TaggedValue,
) -> CandidateValue<FieldValue> {
    let TaggedValue::Some(value) = tagged else {
        return initial.clone();
    };

    let mut candidate = initial.clone();
    match operation {
        Operation::Equals(_, _) => candidate.intersect(CandidateValue::Single(value)),
        Operation::NotEquals(_, _) => candidate.exclude_single_value(&value),
        Operation::LessThan(_, _) => candidate
            .intersect(CandidateValue::Range(Range::with_end(Bound::Excluded(value), nullable))),
        Operation::LessThanOrEqual(_, _) => candidate
            .intersect(CandidateValue::Range(Range::with_end(Bound::Included(value), nullable))),
        Operation::GreaterThan(_, _) => candidate
            .intersect(CandidateValue::Range(Range::with_start(Bound::Excluded(value), nullable))),
        Operation::GreaterThanOrEqual(_, _) => candidate
            .intersect(CandidateValue::Range(Range::with_end(Bound::Included(value), nullable))),
        Operation::OneOf(_, _) => {
            let values = value
                .as_slice()
                .unwrap_or_else(|| {
                    panic!(
                        "field {field_name} of type {field_type} produced an invalid value when resolving @tag: {value:?}"
                    )
                })
                .to_vec();
            candidate.intersect(CandidateValue::Multiple(values));
        }
        _ => unreachable!("unsupported 'operation': {operation:?}"),
    }
    candidate
}
