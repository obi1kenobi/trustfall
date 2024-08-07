use std::{collections::BTreeMap, sync::Arc};

use crate::ir::{
    Argument, FieldRef, FieldValue, IRQueryComponent, Transform, TransformedValue, Vid,
};

use super::{
    execution::QueryCarrier, tags::compute_tag_with_separate_value, Adapter, ContextIterator,
    TaggedValue,
};

pub(super) fn push_transform_argument_tag_values_onto_stack_during_main_query<
    'query,
    AdapterT: Adapter<'query>,
>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    transforms: &[Transform],
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    let tag_func = move |inner_carrier: &mut QueryCarrier,
                         tag: &FieldRef,
                         inner_iterator: ContextIterator<'query, AdapterT::Vertex>|
          -> ContextIterator<'query, AdapterT::Vertex> {
        Box::new(
            // TODO: We should propagate `RESTORE_CONTEXT` here instead of setting it to `true`,
            //       because it might be unnecessary.
            compute_tag_with_separate_value::<AdapterT, true>(
                adapter,
                inner_carrier,
                component,
                current_vid,
                tag,
                inner_iterator,
            )
            .map(|(mut ctx, tag_value)| {
                let value = match tag_value {
                    TaggedValue::NonexistentOptional => FieldValue::Null,
                    TaggedValue::Some(value) => value,
                };
                ctx.values.push(value);
                ctx
            }),
        )
    };

    push_transform_argument_tag_values_onto_stack::<AdapterT>(
        carrier, transforms, tag_func, iterator,
    )
}

/// At different points during query evaluation, we compute tag values differently.
/// The `tag_func` argument allows us to generalize over the tag value computation
/// while reusing the logic for determining which tag values are necessary.
pub(super) fn push_transform_argument_tag_values_onto_stack<'query, AdapterT: Adapter<'query>>(
    carrier: &mut QueryCarrier,
    transforms: &[Transform],
    mut tag_func: impl FnMut(
        &mut QueryCarrier,
        &FieldRef,
        ContextIterator<'query, AdapterT::Vertex>,
    ) -> ContextIterator<'query, AdapterT::Vertex>,
    mut iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    // Ensure any non-immediate operands (like values coming from tags) are pushed
    // onto the each context's stack before we evaluate the transform.
    // We push them on the stack in reverse order, since the stack is LIFO.
    for transform in transforms.iter().rev() {
        match transform {
            Transform::Add(op) | Transform::AddF(op) => match op {
                Argument::Tag(tag, ..) => {
                    iterator = tag_func(carrier, tag, iterator);
                }
                Argument::Variable(..) => {}
            },
            Transform::Sqrt | Transform::Len | Transform::Abs => {
                // No tag arguments here!
            }
        }
    }

    iterator
}

/// Pop and drop any arguments pushed to the values stack that correspond to the given transforms.
///
/// Consider a query like:
/// ```graphql
/// {
///     Example {
///         value @tag(name: "first")
///
///         possibly_nonexistent @optional {
///             value @transform(op: "+", value: ["%first"]) @tag(name: "opt")
///         }
///
///         # ...
///     }
/// }
/// ```
/// Evaluating the `opt` tag here requires that we push the `first` tag's value to the stack.
/// Then, we compute the value of the `value` property, and apply the `+` transform while popping
/// the `first` value off the stack.
///
/// But what if the optional `possibly_nonexistent` edge doesn't exist?
/// Then we've pushed a value onto the stack, but we aren't going to use it since there's nothing
/// to transform. We must explicitly remove that value from the stack, to avoid corrupting
/// the stack's state and impacting downstream operations.
///
/// That's what this function does: it pops all stack arguments that were pushed to satisfy
/// a given slice of transforms.
pub(super) fn drop_unused_transform_arguments(
    transformed_value: &TransformedValue,
    stack: &mut Vec<FieldValue>,
) {
    for transform in &transformed_value.transforms {
        match transform {
            Transform::Add(arg) | Transform::AddF(arg) => match arg {
                Argument::Tag(..) => {
                    // One argument on the stack here.
                    stack.pop().expect("nothing to pop, this is a bug");
                }
                Argument::Variable(..) => {}
            },
            Transform::Sqrt | Transform::Len | Transform::Abs => {}
        }
    }
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
        Transform::Sqrt => apply_sqrt_transform(value),
        Transform::Add(argument) => match argument {
            Argument::Variable(var) => {
                let operand = &variables[&var.variable_name];
                apply_add_transform(value, operand)
            }
            Argument::Tag(..) => {
                let operand = stack.pop().unwrap_or_else(|| {
                    panic!(
                        "empty stack while attempting to resolve transform operand: {transform:?}"
                    )
                });
                apply_add_transform(value, &operand)
            }
        },
        Transform::AddF(argument) => match argument {
            Argument::Variable(var) => {
                let operand = &variables[&var.variable_name];
                apply_fadd_transform(value, operand)
            }
            Argument::Tag(..) => {
                let operand = stack.pop().unwrap_or_else(|| {
                    panic!(
                        "empty stack while attempting to resolve transform operand: {transform:?}"
                    )
                });
                apply_fadd_transform(value, &operand)
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
fn apply_sqrt_transform(value: &FieldValue) -> FieldValue {
    let input = match value {
        FieldValue::Null => return FieldValue::Null,
        FieldValue::Int64(x) => *x as f64,
        FieldValue::Uint64(x) => *x as f64,
        FieldValue::Float64(x) => *x,
        _ => unreachable!("{value:?}"),
    };
    if input.is_sign_negative() {
        FieldValue::NULL
    } else {
        FieldValue::Float64(input.sqrt())
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
        _ => unreachable!("{value:?} {operand:?}"),
    }
}

#[inline]
fn apply_fadd_transform(value: &FieldValue, operand: &FieldValue) -> FieldValue {
    match (value, operand) {
        (FieldValue::Null, _) => FieldValue::Null,
        (_, FieldValue::Null) => FieldValue::Null,
        (FieldValue::Int64(x), FieldValue::Float64(y)) => FieldValue::Float64(y + (*x as f64)),
        (FieldValue::Uint64(x), FieldValue::Float64(y)) => FieldValue::Float64(y + (*x as f64)),
        (FieldValue::Float64(x), FieldValue::Float64(y)) => FieldValue::Float64(x + y),
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
