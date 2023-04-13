use std::{fmt::Debug, ops::Bound, sync::Arc};

use async_graphql_parser::types::Type;

use crate::{
    interpreter::{
        execution::{
            compute_context_field_with_separate_value, compute_fold_specific_field, QueryCarrier,
        },
        hints::Range,
        Adapter, ContextIterator, ContextOutcomeIterator, InterpretedQuery, TaggedValue,
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
