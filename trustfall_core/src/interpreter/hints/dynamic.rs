use std::{fmt::Debug, ops::Bound, sync::Arc};

use async_graphql_parser::types::Type;

use crate::{
    interpreter::{
        execution::{
            compute_context_field_with_separate_value,
            compute_fold_specific_field_with_separate_value, QueryCarrier,
        },
        hints::Range,
        Adapter, ContextIterator, ContextOutcomeIterator, InterpretedQuery, TaggedValue,
        VertexIterator,
    },
    ir::{ContextField, FieldRef, FieldValue, FoldSpecificField, IRQueryComponent, Operation},
};

use super::CandidateValue;

/// Indicates that a property's value is dependent on another value in the query.
///
/// If [`VertexInfo::dynamically_known_property()`](super::VertexInfo::dynamically_known_property)
/// is able to determine a value for the specified property, it returns
/// a [`DynamicallyResolvedValue`]. The specified property's value may be different
/// in different query results, but the way in which it varies can be determined programmatically
/// and can be resolved to a [`CandidateValue`] for each query result.
///
/// ## Example
///
/// The following query fetches emails where the sender also sent a copy of the email
/// to their own address:
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
/// Consider the process of resolving the `recipient` edge. To improve query runtime,
/// our [`Adapter::resolve_neighbors()`] implementation may want to avoid loading _all_ recipients
/// and instead attempt to only load the recipient that matches the sender's address.
///
/// However, as the sender's address varies from email to email, its value must be resolved
/// dynamically, i.e. separately for each possible query result. Resolving the `recipient` edge
/// might then look like this:
/// ```rust
/// # use std::sync::Arc;
/// # use trustfall_core::{
/// #     ir::{EdgeParameters, FieldValue},
/// #     interpreter::{
/// #         Adapter, CandidateValue, ContextIterator, ContextOutcomeIterator,
/// #         ResolveEdgeInfo, ResolveInfo, VertexInfo, VertexIterator,
/// #     },
/// # };
/// # #[derive(Debug, Clone)]
/// # struct Vertex;
/// # struct EmailAdapter;
/// # impl<'a> Adapter<'a> for EmailAdapter {
/// #     type Vertex = Vertex;
/// #
/// #     fn resolve_starting_vertices(
/// #         &self,
/// #         edge_name: &Arc<str>,
/// #         parameters: &EdgeParameters,
/// #         resolve_info: &ResolveInfo,
/// #     ) -> VertexIterator<'a, Self::Vertex> { todo!() }
/// #
/// #     fn resolve_property(
/// #         &self,
/// #         contexts: ContextIterator<'a, Self::Vertex>,
/// #         type_name: &Arc<str>,
/// #         property_name: &Arc<str>,
/// #         resolve_info: &ResolveInfo,
/// #     ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> { todo!() }
/// #
/// #     fn resolve_neighbors(
/// #         &self,
/// #         contexts: ContextIterator<'a, Self::Vertex>,
/// #         type_name: &Arc<str>,
/// #         edge_name: &Arc<str>,
/// #         parameters: &EdgeParameters,
/// #         resolve_info: &ResolveEdgeInfo,
/// #     ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>> { todo!() }
/// #
/// #     fn resolve_coercion(
/// #         &self,
/// #         contexts: ContextIterator<'a, Self::Vertex>,
/// #         type_name: &Arc<str>,
/// #         coerce_to_type: &Arc<str>,
/// #         resolve_info: &ResolveInfo,
/// #     ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> { todo!() }
/// # }
/// #
/// # fn resolve_recipient_from_candidate_value<'a>(
/// #     vertex: &Vertex,
/// #     candidate: CandidateValue<FieldValue>
/// # ) -> VertexIterator<'a, Vertex> {
/// #     todo!()
/// # }
/// #
/// # fn resolve_recipient_otherwise<'a>(
/// #     contexts: ContextIterator<'a, Vertex>,
/// # ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
/// #     todo!()
/// # }
/// #
/// # impl EmailAdapter {
/// // Inside our adapter implementation:
/// // we use this method to resolve `recipient` edges.
/// fn resolve_recipient_edge<'a>(
///     &self,
///     contexts: ContextIterator<'a, Vertex>,
///     resolve_info: &ResolveEdgeInfo,
/// ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
///     if let Some(dynamic_value) = resolve_info.destination().dynamically_known_property("address") {
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

macro_rules! compute_candidate_from_tagged_value {
    ($iterator:ident, $initial_candidate:ident, $candidate:ident, $value:ident, $blk:block) => {
        Box::new($iterator.map(move |(ctx, tagged_value)| {
            let mut $candidate = $initial_candidate.clone();
            match tagged_value {
                TaggedValue::NonexistentOptional => (ctx, $candidate),
                TaggedValue::Some($value) => {
                    {
                        $blk
                    }
                    (ctx, $candidate)
                }
            }
        }))
    };
}

macro_rules! resolve_fold_specific_field {
    ($iterator:ident, $initial_candidate:ident, $candidate:ident, $value:ident, $blk:block) => {
        Box::new($iterator.map(move |(ctx, tagged_value)| {
            let mut $candidate = $initial_candidate.clone();
            if let TaggedValue::Some($value) = tagged_value {
                $blk
            }
            (ctx, $candidate)
        }))
    };
}

impl<'a> DynamicallyResolvedValue<'a> {
    pub(super) fn new(
        query: InterpretedQuery,
        resolve_on_component: &'a IRQueryComponent,
        field: &'a FieldRef,
        operation: Operation<(), ()>,
        initial_candidate: CandidateValue<FieldValue>,
    ) -> Self {
        Self {
            query,
            resolve_on_component,
            field,
            operation,
            initial_candidate,
        }
    }

    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    pub fn resolve<'vertex, AdapterT: Adapter<'vertex>>(
        self,
        adapter: &AdapterT,
        contexts: ContextIterator<'vertex, AdapterT::Vertex>,
    ) -> ContextOutcomeIterator<'vertex, AdapterT::Vertex, CandidateValue<FieldValue>> {
        match &self.field {
            FieldRef::ContextField(context_field) => {
                if context_field.vertex_id < self.resolve_on_component.root {
                    // We're inside at least one level of `@fold` relative to
                    // the origin of this tag.
                    //
                    // We'll have to grab the tag's value from the context directly.
                    let field_ref = self.field;
                    self.compute_candidate_from_tagged_value_with_imported_tags(field_ref, contexts)
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
                    self.compute_candidate_from_tagged_value_with_imported_tags(field_ref, contexts)
                } else {
                    self.resolve_fold_specific_field(fold_field, contexts)
                }
            }
        }
    }

    #[allow(dead_code)] // false-positive: dead in the bin target, not dead in the lib
    pub fn resolve_with<'vertex, AdapterT: Adapter<'vertex>>(
        self,
        adapter: &AdapterT,
        contexts: ContextIterator<'vertex, AdapterT::Vertex>,
        mut neighbor_resolver: impl FnMut(
                &AdapterT::Vertex,
                CandidateValue<FieldValue>,
            ) -> VertexIterator<'vertex, AdapterT::Vertex>
            + 'vertex,
    ) -> ContextOutcomeIterator<'vertex, AdapterT::Vertex, VertexIterator<'vertex, AdapterT::Vertex>>
    {
        Box::new(
            self.resolve(adapter, contexts)
                .map(move |(ctx, candidate)| {
                    let neighbors = match ctx.active_vertex.as_ref() {
                        Some(vertex) => neighbor_resolver(vertex, candidate),
                        None => Box::new(std::iter::empty()),
                    };
                    (ctx, neighbors)
                }),
        )
    }

    fn compute_candidate_from_tagged_value<'vertex, AdapterT: Adapter<'vertex>>(
        self,
        context_field: &'a ContextField,
        adapter: &AdapterT,
        contexts: ContextIterator<'vertex, AdapterT::Vertex>,
    ) -> ContextOutcomeIterator<'vertex, AdapterT::Vertex, CandidateValue<FieldValue>> {
        let mut carrier = QueryCarrier {
            query: Some(self.query),
        };
        let iterator = compute_context_field_with_separate_value(
            adapter,
            &mut carrier,
            self.resolve_on_component,
            context_field,
            contexts,
        );

        let field_name = context_field.field_name.clone();
        let field_type = context_field.field_type.clone();

        compute_candidate_from_operation(
            &self.operation,
            self.initial_candidate,
            field_name,
            field_type,
            iterator,
        )
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
        let iterator = compute_fold_specific_field_with_separate_value(
            fold_field.fold_eid,
            &fold_field.kind,
            contexts,
        );
        let initial_candidate = self.initial_candidate;

        match &self.operation {
            Operation::Equals(_, _) => {
                resolve_fold_specific_field!(iterator, initial_candidate, candidate, value, {
                    candidate.intersect(CandidateValue::Single(value));
                })
            }
            Operation::NotEquals(_, _) => {
                resolve_fold_specific_field!(iterator, initial_candidate, candidate, value, {
                    candidate.exclude_single_value(&value);
                })
            }
            Operation::LessThan(_, _) => {
                resolve_fold_specific_field!(iterator, initial_candidate, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_end(
                        Bound::Excluded(value),
                        false,
                    )));
                })
            }
            Operation::LessThanOrEqual(_, _) => {
                resolve_fold_specific_field!(iterator, initial_candidate, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_end(
                        Bound::Included(value),
                        false,
                    )));
                })
            }
            Operation::GreaterThan(_, _) => {
                resolve_fold_specific_field!(iterator, initial_candidate, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_start(
                        Bound::Excluded(value),
                        false,
                    )));
                })
            }
            Operation::GreaterThanOrEqual(_, _) => {
                resolve_fold_specific_field!(iterator, initial_candidate, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_end(
                        Bound::Included(value),
                        false,
                    )));
                })
            }
            Operation::OneOf(_, _) => {
                let fold_field = fold_field.clone();
                resolve_fold_specific_field!(iterator, initial_candidate, candidate, value, {
                    let values = value
                        .as_slice()
                        .unwrap_or_else(|| {
                            panic!(
                                "\
field {fold_field:?} produced an invalid value when resolving @tag: {value:?}",
                            )
                        })
                        .to_vec();
                    candidate.intersect(CandidateValue::Multiple(values));
                })
            }
            _ => unreachable!(
                "unsupported 'operation' {:?} for tag {:?} in component {:?}",
                &self.operation, fold_field, self.resolve_on_component,
            ),
        }
    }
}

fn compute_candidate_from_operation<'vertex, Vertex: Debug + Clone + 'vertex>(
    operation: &Operation<(), ()>,
    initial_candidate: CandidateValue<FieldValue>,
    field_name: Arc<str>,
    field_type: Type,
    iterator: ContextOutcomeIterator<'vertex, Vertex, TaggedValue>,
) -> ContextOutcomeIterator<'vertex, Vertex, CandidateValue<FieldValue>> {
    match operation {
        Operation::Equals(_, _) => {
            compute_candidate_from_tagged_value!(iterator, initial_candidate, candidate, value, {
                candidate.intersect(CandidateValue::Single(value));
            })
        }
        Operation::NotEquals(_, _) => {
            compute_candidate_from_tagged_value!(iterator, initial_candidate, candidate, value, {
                candidate.exclude_single_value(&value);
            })
        }
        Operation::LessThan(_, _) => {
            compute_candidate_from_tagged_value!(iterator, initial_candidate, candidate, value, {
                candidate.intersect(CandidateValue::Range(Range::with_end(
                    Bound::Excluded(value),
                    true, // nullability is handled in the initial_candidate
                )));
            })
        }
        Operation::LessThanOrEqual(_, _) => {
            compute_candidate_from_tagged_value!(iterator, initial_candidate, candidate, value, {
                candidate.intersect(CandidateValue::Range(Range::with_end(
                    Bound::Included(value),
                    true, // nullability is handled in the initial_candidate
                )));
            })
        }
        Operation::GreaterThan(_, _) => {
            compute_candidate_from_tagged_value!(iterator, initial_candidate, candidate, value, {
                candidate.intersect(CandidateValue::Range(Range::with_start(
                    Bound::Excluded(value),
                    true, // nullability is handled in the initial_candidate
                )));
            })
        }
        Operation::GreaterThanOrEqual(_, _) => {
            compute_candidate_from_tagged_value!(iterator, initial_candidate, candidate, value, {
                candidate.intersect(CandidateValue::Range(Range::with_end(
                    Bound::Included(value),
                    true, // nullability is handled in the initial_candidate
                )));
            })
        }
        Operation::OneOf(_, _) => {
            compute_candidate_from_tagged_value!(iterator, initial_candidate, candidate, value, {
                let values = value
                    .as_slice()
                    .unwrap_or_else(|| {
                        panic!(
                            "\
field {} of type {} produced an invalid value when resolving @tag: {value:?}",
                            field_name, field_type,
                        )
                    })
                    .to_vec();
                candidate.intersect(CandidateValue::Multiple(values));
            })
        }
        _ => unreachable!("unsupported 'operation': {:?}", operation,),
    }
}
