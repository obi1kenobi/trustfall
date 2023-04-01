use std::{fmt::Debug, ops::Bound};

use crate::{
    interpreter::{
        execution::{compute_context_field_with_separate_value, QueryCarrier},
        hints::Range,
        Adapter, ContextIterator, ContextOutcomeIterator, InterpretedQuery,
    },
    ir::{ContextField, FieldRef, FieldValue, IRQueryComponent, Operation},
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

macro_rules! resolve_operation {
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

    pub fn resolve<
        'vertex,
        VertexT: Debug + Clone + 'vertex,
        AdapterT: Adapter<'vertex, Vertex = VertexT>,
    >(
        self,
        adapter: &mut AdapterT,
        contexts: ContextIterator<'vertex, VertexT>,
    ) -> ContextOutcomeIterator<'vertex, VertexT, CandidateValue<FieldValue>> {
        match self.field {
            FieldRef::ContextField(context_field) => {
                self.resolve_context_field(context_field, adapter, contexts)
            }
            FieldRef::FoldSpecificField(fold_field) => todo!(),
        }
    }

    fn resolve_context_field<
        'vertex,
        VertexT: Debug + Clone + 'vertex,
        AdapterT: Adapter<'vertex, Vertex = VertexT>,
    >(
        self,
        context_field: &'a ContextField,
        adapter: &mut AdapterT,
        contexts: ContextIterator<'vertex, VertexT>,
    ) -> ContextOutcomeIterator<'vertex, VertexT, CandidateValue<FieldValue>> {
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
                resolve_operation!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    candidate.intersect(CandidateValue::Single(value));
                })
            }
            Operation::NotEquals(_, _) => {
                resolve_operation!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    candidate.exclude_single_value(&value);
                })
            }
            Operation::LessThan(_, _) => {
                resolve_operation!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_end(
                        Bound::Excluded(value),
                        nullable_context_field,
                    )));
                })
            }
            Operation::LessThanOrEqual(_, _) => {
                resolve_operation!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_end(
                        Bound::Included(value),
                        nullable_context_field,
                    )));
                })
            }
            Operation::GreaterThan(_, _) => {
                resolve_operation!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_start(
                        Bound::Excluded(value),
                        nullable_context_field,
                    )));
                })
            }
            Operation::GreaterThanOrEqual(_, _) => {
                resolve_operation!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    candidate.intersect(CandidateValue::Range(Range::with_end(
                        Bound::Included(value),
                        nullable_context_field,
                    )));
                })
            }
            Operation::OneOf(_, _) => {
                let context_field_name = context_field.field_name.clone();
                let context_field_type = context_field.field_type.clone();
                resolve_operation!(iterator, initial_candidate, ctx_vid, candidate, value, {
                    let values = value
                        .as_slice()
                        .unwrap_or_else(|| {
                            panic!(
                                "\
field {} of type {:?} produced an invalid value when resolving @tag: {value:?}",
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
}
