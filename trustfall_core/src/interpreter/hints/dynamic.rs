use std::{fmt::Debug, ops::Bound};

use crate::{
    interpreter::{
        execution::{
            compute_context_field_with_separate_value, compute_fold_specific_field, QueryCarrier,
        },
        hints::Range,
        Adapter, ContextIterator, ContextOutcomeIterator, InterpretedQuery,
    },
    ir::{ContextField, FieldRef, FieldValue, FoldSpecificField, IRQueryComponent, Operation},
};

use super::CandidateValue;

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicallyResolvedValue<'a> {
    query: InterpretedQuery,
    resolve_on_component: &'a IRQueryComponent,
    field: &'a FieldRef,
    operation: Operation<(), ()>,
    initial_candidate: CandidateValue<FieldValue>,
}

macro_rules! resolve_context_field {
    ($iterator:ident, $initial_candidate:ident, $context_field_vid:ident, $candidate:ident, $value:ident, $blk:block) => {
        Box::new($iterator.map(move |(ctx, $value)| {
            let mut $candidate = $initial_candidate.clone();
            // Check if the tag was from an @optional scope that didn't exist.
            // In such a case, our query rules say that any filters using that tag
            // *must* pass, so we don't constrain candidate values any further.
            if ctx.vertices[&$context_field_vid].is_some() {
                $blk
            };
            (ctx, $candidate)
        }))
    };
}

macro_rules! resolve_fold_field {
    ($iterator:ident, $initial_candidate:ident, $candidate:ident, $value:ident, $blk:block) => {
        Box::new($iterator.map(move |mut ctx| {
            let $value = ctx.values.pop().expect("no value in values() stack");
            let mut $candidate = $initial_candidate.clone();
            $blk(ctx, $candidate)
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
                self.resolve_context_field(context_field, adapter, contexts)
            }
            FieldRef::FoldSpecificField(fold_field) => {
                // TODO cover this with tests
                if fold_field.fold_root_vid < self.resolve_on_component.root {
                    // We're inside at least one level of `@fold` relative to
                    // the origin of this tag.
                    //
                    // We'll have to grab the tag's value from the context directly.
                    let field_ref = self.field;
                    self.resolve_context_field_with_imported_tags(field_ref, contexts)
                } else {
                    self.resolve_fold_specific_field(fold_field, contexts)
                }
            }
        }
    }

    fn resolve_context_field<'vertex, AdapterT: Adapter<'vertex>>(
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
        let ctx_vid = context_field.vertex_id;
        let nullable_context_field = context_field.field_type.nullable;
        let initial_candidate = self.initial_candidate;

        match &self.operation {
            Operation::Equals(_, _) => {
                resolve_context_field!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    candidate.intersect(CandidateValue::Single(value));
                })
            }
            Operation::NotEquals(_, _) => {
                resolve_context_field!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    candidate.exclude_single_value(&value);
                })
            }
            Operation::LessThan(_, _) => {
                dbg!(ctx_vid);
                dbg!(self.resolve_on_component.root);
                resolve_context_field!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_end(
                        Bound::Excluded(value),
                        nullable_context_field,
                    )));
                })
            }
            Operation::LessThanOrEqual(_, _) => {
                resolve_context_field!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_end(
                        Bound::Included(value),
                        nullable_context_field,
                    )));
                })
            }
            Operation::GreaterThan(_, _) => {
                resolve_context_field!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_start(
                        Bound::Excluded(value),
                        nullable_context_field,
                    )));
                })
            }
            Operation::GreaterThanOrEqual(_, _) => {
                resolve_context_field!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_end(
                        Bound::Included(value),
                        nullable_context_field,
                    )));
                })
            }
            Operation::OneOf(_, _) => {
                let context_field_name = context_field.field_name.clone();
                let context_field_type = context_field.field_type.clone();
                resolve_context_field!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    let values = value
                        .as_slice()
                        .unwrap_or_else(|| {
                            panic!(
                                "\
field {} of type {} produced an invalid value when resolving @tag: {value:?}",
                                context_field_name, context_field_type,
                            )
                        })
                        .to_vec();
                    candidate.intersect(CandidateValue::Multiple(values));
                })
            }
            _ => unreachable!(
                "unsupported 'operation' {:?} for tag {:?} in component {:?}",
                &self.operation, context_field, self.resolve_on_component,
            ),
        }
    }

    fn resolve_context_field_with_imported_tags<'vertex, VertexT: Debug + Clone + 'vertex>(
        self,
        field_ref: &'a FieldRef,
        contexts: ContextIterator<'vertex, VertexT>,
    ) -> ContextOutcomeIterator<'vertex, VertexT, CandidateValue<FieldValue>> {
        let initial_candidate = self.initial_candidate;
        let nullable_context_field = match field_ref {
            FieldRef::ContextField(ctx) => ctx.field_type.nullable,
            FieldRef::FoldSpecificField(_) => false,
        };
        let cloned_field_ref = field_ref.clone();
        let iterator = contexts.map(move |ctx| {
            let value = ctx.imported_tags[&cloned_field_ref].clone();
            (ctx, value)
        });

        match &self.operation {
            Operation::Equals(_, _) => Box::new(iterator.map(move |(ctx, value)| {
                let mut candidate = initial_candidate.clone();
                candidate.intersect(CandidateValue::Single(value));
                (ctx, candidate)
            })),
            Operation::NotEquals(_, _) => Box::new(iterator.map(move |(ctx, value)| {
                let mut candidate = initial_candidate.clone();
                candidate.exclude_single_value(&value);
                (ctx, candidate)
            })),
            Operation::LessThan(_, _) => Box::new(iterator.map(move |(ctx, value)| {
                let mut candidate = initial_candidate.clone();
                candidate.intersect(CandidateValue::Range(Range::with_end(
                    Bound::Excluded(value),
                    nullable_context_field,
                )));
                (ctx, candidate)
            })),
            Operation::LessThanOrEqual(_, _) => Box::new(iterator.map(move |(ctx, value)| {
                let mut candidate = initial_candidate.clone();
                candidate.intersect(CandidateValue::Range(Range::with_end(
                    Bound::Included(value),
                    nullable_context_field,
                )));
                (ctx, candidate)
            })),
            Operation::GreaterThan(_, _) => Box::new(iterator.map(move |(ctx, value)| {
                let mut candidate = initial_candidate.clone();
                candidate.intersect(CandidateValue::Range(Range::with_start(
                    Bound::Excluded(value),
                    nullable_context_field,
                )));
                (ctx, candidate)
            })),
            Operation::GreaterThanOrEqual(_, _) => Box::new(iterator.map(move |(ctx, value)| {
                let mut candidate = initial_candidate.clone();
                candidate.intersect(CandidateValue::Range(Range::with_end(
                    Bound::Included(value),
                    nullable_context_field,
                )));
                (ctx, candidate)
            })),
            Operation::OneOf(_, _) => {
                let field_ref = field_ref.clone();
                Box::new(iterator.map(move |(ctx, value)| {
                    let mut candidate = initial_candidate.clone();
                    let values = value
                        .as_slice()
                        .unwrap_or_else(|| match &field_ref {
                            FieldRef::ContextField(context_field) => {
                                panic!(
                                    "\
field {} of type {} produced an invalid value when resolving @tag: {value:?}",
                                    context_field.field_name, context_field.field_type,
                                )
                            }
                            FieldRef::FoldSpecificField(fold_specific) => {
                                panic!(
                                    "\
field {:?} produced an invalid value when resolving @tag: {value:?}",
                                    fold_specific,
                                )
                            }
                        })
                        .to_vec();
                    candidate.intersect(CandidateValue::Multiple(values));
                    (ctx, candidate)
                }))
            }
            _ => unreachable!(
                "unsupported 'operation' {:?} for tag {:?} in component {:?}",
                &self.operation, field_ref, self.resolve_on_component,
            ),
        }
    }

    fn resolve_fold_specific_field<'vertex, VertexT: Debug + Clone + 'vertex>(
        self,
        fold_field: &'a FoldSpecificField,
        contexts: ContextIterator<'vertex, VertexT>,
    ) -> ContextOutcomeIterator<'vertex, VertexT, CandidateValue<FieldValue>> {
        let iterator = compute_fold_specific_field(fold_field.fold_eid, &fold_field.kind, contexts);
        let initial_candidate = self.initial_candidate;

        match &self.operation {
            Operation::Equals(_, _) => {
                resolve_fold_field!(iterator, initial_candidate, candidate, value, {
                    candidate.intersect(CandidateValue::Single(value));
                })
            }
            Operation::NotEquals(_, _) => {
                resolve_fold_field!(iterator, initial_candidate, candidate, value, {
                    candidate.exclude_single_value(&value);
                })
            }
            Operation::LessThan(_, _) => {
                resolve_fold_field!(iterator, initial_candidate, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_end(
                        Bound::Excluded(value),
                        false,
                    )));
                })
            }
            Operation::LessThanOrEqual(_, _) => {
                resolve_fold_field!(iterator, initial_candidate, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_end(
                        Bound::Included(value),
                        false,
                    )));
                })
            }
            Operation::GreaterThan(_, _) => {
                resolve_fold_field!(iterator, initial_candidate, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_start(
                        Bound::Excluded(value),
                        false,
                    )));
                })
            }
            Operation::GreaterThanOrEqual(_, _) => {
                resolve_fold_field!(iterator, initial_candidate, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_end(
                        Bound::Included(value),
                        false,
                    )));
                })
            }
            Operation::OneOf(_, _) => {
                let fold_field = fold_field.clone();
                resolve_fold_field!(iterator, initial_candidate, candidate, value, {
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
