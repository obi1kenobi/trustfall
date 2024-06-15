use std::{collections::BTreeMap, sync::Arc};

use crate::ir::{Argument, FieldValue, IRQueryComponent, Transform, TransformedValue, Vid};

use super::{
    execution::QueryCarrier, tags::compute_tag_with_separate_value, Adapter, ContextIterator,
    TaggedValue,
};

pub(super) fn push_transform_argument_tag_values_onto_stack<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    transforms: &[Transform],
    mut iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    // Ensure any non-immediate operands (like values coming from tags) are pushed
    // onto the each context's stack before we evaluate the transform.
    // We push them on the stack in reverse order, since the stack is LIFO.
    for transform in transforms.iter().rev() {
        match transform {
            Transform::Add(op) => match op {
                Argument::Tag(tag) => {
                    iterator = Box::new(
                        compute_tag_with_separate_value(
                            adapter,
                            carrier,
                            component,
                            current_vid,
                            tag,
                            iterator,
                        )
                        .map(|(mut ctx, tag_value)| {
                            let value = match tag_value {
                                TaggedValue::NonexistentOptional => FieldValue::Null,
                                TaggedValue::Some(value) => value,
                            };
                            ctx.values.push(value);
                            ctx
                        }),
                    );
                }
                Argument::Variable(..) => {}
            },
            Transform::Len | Transform::Abs => {
                // No tag arguments here!
            }
        }
    }

    iterator
}

pub(super) fn apply_transforms(
    transformed_value: &TransformedValue,
    variables: &BTreeMap<Arc<str>, FieldValue>,
    stack: &mut Vec<FieldValue>,
    mut value: FieldValue,
) -> FieldValue {
    for transform in &transformed_value.transforms {
        value = apply_one_transform(transform, variables, stack, &value);
    }

    value
}

#[inline]
fn apply_one_transform(
    transform: &Transform,
    variables: &BTreeMap<Arc<str>, FieldValue>,
    stack: &mut Vec<FieldValue>,
    value: &FieldValue,
) -> FieldValue {
    match transform {
        Transform::Len => apply_len_transform(value),
        Transform::Abs => apply_abs_transform(value),
        Transform::Add(argument) => match argument {
            Argument::Variable(var) => {
                let operand = &variables[&var.variable_name];
                apply_add_transform(value, operand)
            }
            Argument::Tag(_) => {
                let operand = stack.pop().expect(
                    "empty stack while attempting to resolve transform operand: {transform:?}",
                );
                apply_add_transform(value, &operand)
            }
        },
    }
}

#[inline]
fn apply_len_transform(value: &FieldValue) -> FieldValue {
    match value {
        FieldValue::Null => FieldValue::Null,
        FieldValue::List(l) => FieldValue::Int64(l.len() as i64),
        _ => unreachable!("{value:?}"),
    }
}

#[inline]
fn apply_abs_transform(value: &FieldValue) -> FieldValue {
    match value {
        FieldValue::Null => FieldValue::Null,
        FieldValue::Int64(x) => FieldValue::Uint64(x.unsigned_abs()),
        FieldValue::Uint64(x) => FieldValue::Uint64(*x),
        FieldValue::Float64(x) => FieldValue::Float64(x.abs()),
        _ => unreachable!("{value:?}"),
    }
}

#[inline]
fn apply_add_transform(value: &FieldValue, operand: &FieldValue) -> FieldValue {
    match (value, operand) {
        (FieldValue::Null, _) => FieldValue::Null,
        (_, FieldValue::Null) => FieldValue::Null,
        (FieldValue::Int64(x), FieldValue::Int64(y)) => FieldValue::Int64(x.saturating_add(*y)),
        (FieldValue::Uint64(x), FieldValue::Uint64(y)) => FieldValue::Uint64(x.saturating_add(*y)),
        (FieldValue::Int64(signed), FieldValue::Uint64(unsigned))
        | (FieldValue::Uint64(unsigned), FieldValue::Int64(signed)) => {
            add_unlike_signedness_integers(*signed, *unsigned)
        }
        (FieldValue::Float64(x), FieldValue::Float64(y)) => FieldValue::Float64(x + y),
        (FieldValue::Float64(x), FieldValue::Int64(y))
        | (FieldValue::Int64(y), FieldValue::Float64(x)) => FieldValue::Float64(x + (*y as f64)),
        (FieldValue::Float64(x), FieldValue::Uint64(y))
        | (FieldValue::Uint64(y), FieldValue::Float64(x)) => FieldValue::Float64(x + (*y as f64)),
        _ => unreachable!("{value:?} {operand:?}"),
    }
}

#[inline]
fn add_unlike_signedness_integers(signed: i64, unsigned: u64) -> FieldValue {
    if (unsigned > i64::MAX as u64) || !signed.is_negative() {
        return FieldValue::Uint64(unsigned.saturating_add_signed(signed));
    }

    FieldValue::Int64(signed.saturating_add_unsigned(unsigned))
}

#[cfg(test)]
mod tests {
    use crate::ir::FieldValue;

    use super::add_unlike_signedness_integers;

    #[test]
    fn test_add_unlike_signedness_integers() {
        let test_data = [
            // Adding two non-negative numbers results in a u64.
            (123i64, 456u64, FieldValue::Uint64(579)),
            (i64::MAX, 0, FieldValue::Uint64(i64::MAX as u64)),
            (i64::MAX, 1, FieldValue::Uint64(i64::MAX as u64 + 1)),
            // Adding a negative and positive number far from the numeric bounds results in i64.
            (-123, 122, FieldValue::Int64(-1)),
            (-123, 123, FieldValue::Int64(0)),
            (-123, 124, FieldValue::Int64(1)),
            // Adding a small negative number to a u64 above the i64 numeric bound results in u64.
            (-1, u64::MAX, FieldValue::Uint64(u64::MAX - 1)),
            // Addition right up to the numeric bounds.
            (i64::MAX, u64::MAX - (i64::MAX as u64), FieldValue::Uint64(u64::MAX)),
            (i64::MIN, 0, FieldValue::Int64(i64::MIN)),
            // Saturation at the numeric bounds instead of overflow or underflow.
            (i64::MAX, u64::MAX, FieldValue::Uint64(u64::MAX)),
        ];

        for (signed, unsigned, expected) in test_data {
            let actual = add_unlike_signedness_integers(signed, unsigned);
            assert_eq!(
                expected, actual,
                "{signed} + {unsigned} => {actual:?} but expected {expected:?}"
            );
            assert!(
                expected.structural_eq(&actual),
                "values compare equal but are structurally different: {expected:?} {actual:?}"
            );
        }
    }
}
