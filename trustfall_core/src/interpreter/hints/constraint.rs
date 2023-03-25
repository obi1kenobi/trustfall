use std::{fmt::Debug, sync::Arc};

use crate::{
    interpreter::{
        execution::compute_context_field_with_separate_value, Adapter, ContextIterator,
        ContextOutcomeIterator, QueryInfo,
    },
    ir::{ContextField, FieldValue, IRQueryComponent},
};

use super::CandidateValue;

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DynamicConstraint<C: Debug + Clone + PartialEq + Eq> {
    resolve_on_component: Arc<IRQueryComponent>,
    field: ContextField,
    content: C,
}

impl<C: Debug + Clone + PartialEq + Eq> DynamicConstraint<C> {
    pub(super) fn new(
        resolve_on_component: Arc<IRQueryComponent>,
        field: ContextField,
        content: C,
    ) -> Self {
        Self {
            resolve_on_component,
            field,
            content,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BoundKind {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CandidateInfo {
    pub(super) is_multiple: bool,
}

impl CandidateInfo {
    pub(super) fn new(is_multiple: bool) -> Self {
        Self { is_multiple }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicallyResolvedValue {
    query_info: QueryInfo,
    constraint: DynamicConstraint<CandidateInfo>,
}

impl DynamicallyResolvedValue {
    pub(super) fn new(query_info: QueryInfo, constraint: DynamicConstraint<CandidateInfo>) -> Self {
        Self {
            query_info,
            constraint,
        }
    }

    pub fn resolve<
        'vertex,
        VertexT: Debug + Clone + 'vertex,
        AdapterT: Adapter<'vertex, Vertex = VertexT>,
    >(
        mut self,
        adapter: &mut AdapterT,
        contexts: ContextIterator<'vertex, VertexT>,
    ) -> ContextOutcomeIterator<'vertex, VertexT, CandidateValue<FieldValue>> {
        let iterator = compute_context_field_with_separate_value(
            adapter,
            &mut self.query_info,
            &self.constraint.resolve_on_component,
            &self.constraint.field,
            contexts,
        );
        let context_field_vid = self.constraint.field.vertex_id;
        let nullable_context_field = self.constraint.field.field_type.nullable;
        if self.constraint.content.is_multiple {
            Box::new(iterator.map(move |(ctx, value)| {
                match value {
                    FieldValue::List(v) => (ctx, CandidateValue::Multiple(v)),
                    FieldValue::Null => {
                        // Either a nullable field was tagged, or
                        // the @tag is inside an @optional scope that doesn't exist.
                        let candidate = if ctx.vertices[&context_field_vid].is_none() {
                            // @optional scope that didn't exist. Our query rules say that
                            // any filters using this tag *must* pass.
                            CandidateValue::All
                        } else {
                            // The field must have been nullable.
                            debug_assert!(
                                nullable_context_field,
                                "tagged field {:?} was not nullable but received a null value for it",
                                self.constraint.field,
                            );
                            CandidateValue::Impossible
                        };
                        (ctx, candidate)
                    }
                    bad_value => {
                        panic!(
                            "\
field {} of type {:?} produced an invalid value when resolving @tag: {bad_value:?}",
                            self.constraint.field.field_name, self.constraint.field.field_type,
                        )
                    }
                }
            }))
        } else {
            Box::new(iterator.map(move |(ctx, value)| match value {
                null_value @ FieldValue::Null => {
                    // Either a nullable field was tagged, or
                    // the @tag is inside an @optional scope that doesn't exist.
                    let candidate = if ctx.vertices[&context_field_vid].is_none() {
                        // @optional scope that didn't exist. Our query rules say that
                        // any filters using this tag *must* pass.
                        CandidateValue::All
                    } else {
                        // The field must have been nullable.
                        debug_assert!(
                            nullable_context_field,
                            "tagged field {:?} was not nullable but received a null value for it",
                            self.constraint.field,
                        );
                        CandidateValue::Single(null_value)
                    };
                    (ctx, candidate)
                }
                other_value => (ctx, CandidateValue::Single(other_value)),
            }))
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicallyResolvedRange {
    info: QueryInfo,
    constraint: DynamicConstraint<BoundKind>,
}

impl DynamicallyResolvedRange {
    pub(super) fn new(info: QueryInfo, constraint: DynamicConstraint<BoundKind>) -> Self {
        Self { info, constraint }
    }
}
