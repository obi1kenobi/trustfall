use std::{collections::BTreeMap, ops::Bound, sync::Arc};

use itertools::Itertools;

use crate::ir::{
    Argument, FieldValue, IREdge, IRFold, IRQueryComponent, IRVertex, LocalField, Operation, Vid,
};

use super::{candidates::NullableValue, CandidateValue, EdgeInfo, Range, dynamic::DynamicallyResolvedValue};

/// Information about what the currently-executing query needs at a specific vertex.
#[cfg_attr(docsrs, doc(notable_trait))]
pub trait VertexInfo: super::sealed::__Sealed {
    /// The unique ID of the vertex this [`VertexInfo`] describes.
    fn vid(&self) -> Vid;

    /// The type coercion (`... on SomeType`) applied by the query at this vertex, if any.
    fn coerced_to_type(&self) -> Option<&Arc<str>>;

    /// Check whether the query demands this vertex property to be in a known set of values,
    /// where this set is known *statically* i.e. up-front and without executing any of the query.
    ///
    /// For example, filtering a property with a query argument like
    /// `@filter(op: "=", value: ["$expected"])` means the filtered property will
    /// need to match the value of the `expected` query variable.
    fn statically_known_property(&self, name: &str) -> Option<CandidateValue<&FieldValue>>;

    /// Check whether the query demands this vertex property to be in a known set of values,
    /// where this set is known *dynamically* i.e. requires some of the query
    /// to have already been executed at the point when this method is called.
    ///
    /// For example, filtering a property with `@filter(op: "=", value: ["%expected"])`
    /// means the property must have a value equal to the value of an earlier property
    /// whose value is tagged like `@tag(name: "expected")`. If the vertex containing
    /// the tagged property has already been computed in this query, this method will offer
    /// to dynamically resolve the tagged value.
    ///
    /// Candidate values produced via this method already reflect all statically-known information
    /// about the property. Calling [`VertexInfo::statically_known_property()`] in addition
    /// to this method is not necessary.
    ///
    /// TODO: this returns `None` even if there's a valid tag with a supported filter,
    ///       if the tag's vertex *has not been resolved yet* -- make sure to test this!!
    fn dynamically_known_property(&self, name: &str) -> Option<DynamicallyResolvedValue>;

    /// Returns info for the first edge by the given name that is *mandatory*:
    /// this vertex must contain the edge, or its result set will be discarded.
    ///
    /// Edges marked `@optional`, `@fold`, or `@recurse` are not mandatory:
    /// - `@optional` edges that don't exist produce `null` outputs.
    /// - `@fold` edges that don't exist produce empty aggregations.
    /// - `@recurse` always starts at depth 0 (i.e. returning the *current* vertex),
    ///   so the edge is not required to exist.
    ///
    /// Just a convenience wrapper over [`VertexInfo::edges_with_name()`].
    fn first_mandatory_edge(&self, name: &str) -> Option<EdgeInfo>;

    /// Returns info for the first edge by the given name.
    ///
    /// Just a convenience wrapper over [`VertexInfo::edges_with_name()`].
    fn first_edge(&self, name: &str) -> Option<EdgeInfo>;

    /// Returns an iterator of all the edges by that name being resolved from this vertex.
    ///
    /// This is the building block of [`VertexInfo::first_edge()`] and
    /// [`VertexInfo::first_mandatory_edge()`].
    /// When possible, prefer using those methods as they are much simpler to understand.
    fn edges_with_name<'a>(&'a self, name: &'a str) -> Box<dyn Iterator<Item = EdgeInfo> + 'a>;
}

pub(super) trait InternalVertexInfo: super::sealed::__Sealed {
    fn current_vertex(&self) -> &IRVertex;

    fn current_component(&self) -> &IRQueryComponent;

    fn query_variables(&self) -> &BTreeMap<Arc<str>, FieldValue>;

    fn make_non_folded_edge_info(&self, edge: &IREdge) -> EdgeInfo;

    fn make_folded_edge_info(&self, fold: &IRFold) -> EdgeInfo;
}

impl<T: InternalVertexInfo + super::sealed::__Sealed> VertexInfo for T {
    fn vid(&self) -> Vid {
        self.current_vertex().vid
    }

    fn coerced_to_type(&self) -> Option<&Arc<str>> {
        let vertex = self.current_vertex();
        if vertex.coerced_from_type.is_some() {
            Some(&vertex.type_name)
        } else {
            None
        }
    }

    fn statically_known_property(&self, property: &str) -> Option<CandidateValue<&FieldValue>> {
        let query_variables = self.query_variables();

        // We only care about filtering operations that are both:
        // - on the requested property of this vertex, and
        // - statically-resolvable, i.e. do not depend on tagged arguments
        let mut relevant_filters = filters_on_local_property(self.current_vertex(), property)
            .filter(|op| {
                // Either there's no "right-hand side" in the operator (as in "is_not_null"),
                // or the right-hand side is a variable.
                matches!(op.right(), None | Some(Argument::Variable(..)))
            })
            .peekable();

        // Early-return in case there are no filters that apply here.
        let field = relevant_filters.peek()?.left();

        let candidate =
            compute_statically_known_candidate(field, relevant_filters, query_variables);
        debug_assert!(
            // Ensure we never return a range variant with a completely unrestricted range.
            candidate.as_ref().unwrap_or(&CandidateValue::All) != &CandidateValue::Range(Range::full()),
            "caught returning a range variant with a completely unrestricted range; it should have been CandidateValue::All instead"
        );

        candidate
    }

    fn dynamically_known_property(&self, name: &str) -> Option<DynamicallyResolvedValue> {
        todo!()
    }

    fn edges_with_name<'a>(&'a self, name: &'a str) -> Box<dyn Iterator<Item = EdgeInfo> + 'a> {
        let component = self.current_component();
        let current_vid = self.current_vertex().vid;

        let non_folded_edges = component
            .edges
            .values()
            .filter(move |edge| edge.from_vid == current_vid && edge.edge_name.as_ref() == name)
            .map(|edge| self.make_non_folded_edge_info(edge.as_ref()));
        let folded_edges = component
            .folds
            .values()
            .filter(move |fold| fold.from_vid == current_vid && fold.edge_name.as_ref() == name)
            .map(|fold| self.make_folded_edge_info(fold.as_ref()));

        Box::new(non_folded_edges.chain(folded_edges))
    }

    fn first_mandatory_edge(&self, name: &str) -> Option<EdgeInfo> {
        self.edges_with_name(name)
            .find(|edge| !edge.folded && !edge.optional && edge.recursive.is_none())
    }

    fn first_edge(&self, name: &str) -> Option<EdgeInfo> {
        self.edges_with_name(name).next()
    }
}

fn filters_on_local_property<'a>(
    vertex: &'a IRVertex,
    property_name: &'a str,
) -> impl Iterator<Item = &'a Operation<LocalField, Argument>> {
    vertex
        .filters
        .iter()
        .filter(move |op| op.left().field_name.as_ref() == property_name)
}

fn compute_statically_known_candidate<'a, 'b>(
    field: &'a LocalField,
    relevant_filters: impl Iterator<Item = &'a Operation<LocalField, Argument>>,
    query_variables: &'b BTreeMap<Arc<str>, FieldValue>,
) -> Option<CandidateValue<&'b FieldValue>> {
    let (candidates, post_processing_filters): (Vec<_>, Vec<&Operation<_, _>>) = relevant_filters
        .partition_map(|op| match op {
            Operation::IsNull(..) => {
                itertools::Either::Left(CandidateValue::Single(&FieldValue::NULL))
            }
            Operation::IsNotNull(..) => {
                itertools::Either::Left(CandidateValue::Range(Range::full_non_null()))
            }
            Operation::Equals(_, Argument::Variable(var)) => itertools::Either::Left(
                CandidateValue::Single(&query_variables[var.variable_name.as_ref()]),
            ),
            Operation::LessThan(_, Argument::Variable(var)) => {
                itertools::Either::Left(CandidateValue::Range(Range::with_end(
                    Bound::Excluded(&query_variables[var.variable_name.as_ref()]),
                    true,
                )))
            }
            Operation::LessThanOrEqual(_, Argument::Variable(var)) => {
                itertools::Either::Left(CandidateValue::Range(Range::with_end(
                    Bound::Included(&query_variables[var.variable_name.as_ref()]),
                    true,
                )))
            }
            Operation::GreaterThan(_, Argument::Variable(var)) => {
                itertools::Either::Left(CandidateValue::Range(Range::with_start(
                    Bound::Excluded(&query_variables[var.variable_name.as_ref()]),
                    true,
                )))
            }
            Operation::GreaterThanOrEqual(_, Argument::Variable(var)) => {
                itertools::Either::Left(CandidateValue::Range(Range::with_start(
                    Bound::Included(&query_variables[var.variable_name.as_ref()]),
                    true,
                )))
            }
            Operation::OneOf(_, Argument::Variable(var)) => {
                itertools::Either::Left(CandidateValue::Multiple(
                    query_variables[var.variable_name.as_ref()]
                        .as_vec(Option::Some)
                        .expect("query variable was not list-typed"),
                ))
            }
            Operation::NotEquals(_, Argument::Variable(var))
                if query_variables[var.variable_name.as_ref()].is_null() =>
            {
                // Special case: `!= null` can generate candidates;
                // it's the only `!=` operand for which this is true.
                itertools::Either::Left(CandidateValue::Range(Range::full_non_null()))
            }
            _ => itertools::Either::Right(op),
        });

    let mut candidate = if candidates.is_empty() {
        // No valid candidate-producing filters found.
        return None;
    } else {
        let initial_candidate = if field.field_type.nullable {
            CandidateValue::All
        } else {
            CandidateValue::Range(Range::full_non_null())
        };
        candidates
            .into_iter()
            .fold(initial_candidate, |mut acc, e| {
                acc.intersect(e);
                acc
            })
    };
    candidate.normalize();

    // If we have any filters that may affect the candidate value in post-processing,
    // get their operand values now.
    if post_processing_filters.is_empty() {
        return Some(candidate);
    }
    let filters_and_values: Vec<_> = post_processing_filters
        .into_iter()
        .filter_map(|op| {
            op.right().and_then(|arg| {
                if let Argument::Variable(var) = arg {
                    Some((op, &query_variables[var.variable_name.as_ref()]))
                } else {
                    None
                }
            })
        })
        .collect();

    // Ensure the candidate isn't any value that is directly disallowed by
    // a `!=` or equivalent filter operation.
    let disallowed_values = filters_and_values
        .iter()
        .filter_map(
            |(op, value)| -> Option<Box<dyn Iterator<Item = &FieldValue>>> {
                match op {
                    Operation::NotEquals(..) => Some(Box::new(std::iter::once(*value))),
                    Operation::NotOneOf(..) => Some(Box::new(
                        value
                            .as_slice()
                            .expect("not_one_of operand was not a list")
                            .iter(),
                    )),
                    _ => None,
                }
            },
        )
        .flatten();
    for disallowed in disallowed_values {
        candidate.exclude_single_value(&disallowed);
    }

    // TODO: use the other kinds of filters to exclude more candidate values

    Some(candidate)
}

#[cfg(test)]
mod tests {
    use std::{ops::Bound, sync::Arc};

    use async_graphql_parser::types::Type;

    use crate::{
        interpreter::hints::{
            vertex_info::compute_statically_known_candidate, CandidateValue, Range,
        },
        ir::{Argument, FieldValue, LocalField, Operation, VariableRef},
    };

    #[test]
    fn exclude_not_equals_candidates() {
        let first: Arc<str> = Arc::from("first");
        let second: Arc<str> = Arc::from("second");
        let third: Arc<str> = Arc::from("third");
        let null: Arc<str> = Arc::from("null");
        let list: Arc<str> = Arc::from("my_list");
        let longer_list: Arc<str> = Arc::from("longer_list");
        let nullable_int_type = Type::new("Int").unwrap();
        let int_type = Type::new("Int!").unwrap();
        let list_int_type = Type::new("[Int!]!").unwrap();

        let first_var = Argument::Variable(VariableRef {
            variable_name: first.clone(),
            variable_type: int_type.clone(),
        });
        let second_var = Argument::Variable(VariableRef {
            variable_name: second.clone(),
            variable_type: int_type.clone(),
        });
        let null_var = Argument::Variable(VariableRef {
            variable_name: null.clone(),
            variable_type: nullable_int_type.clone(),
        });
        let list_var = Argument::Variable(VariableRef {
            variable_name: list.clone(),
            variable_type: list_int_type.clone(),
        });
        let longer_list_var = Argument::Variable(VariableRef {
            variable_name: longer_list.clone(),
            variable_type: list_int_type.clone(),
        });

        let local_field = LocalField {
            field_name: Arc::from("my_field"),
            field_type: nullable_int_type.clone(),
        };

        let variables = btreemap! {
            first => FieldValue::Int64(1),
            second => FieldValue::Int64(2),
            third => FieldValue::Int64(3),
            null => FieldValue::Null,
            list => FieldValue::List(vec![FieldValue::Int64(1), FieldValue::Int64(2)]),
            longer_list => FieldValue::List(vec![FieldValue::Int64(1), FieldValue::Int64(2), FieldValue::Int64(3)]),
        };

        let test_data = [
            // Both `= 1` and `!= 1` are impossible to satisfy simultaneously.
            (
                vec![
                    Operation::NotEquals(local_field.clone(), first_var.clone()),
                    Operation::Equals(local_field.clone(), first_var.clone()),
                ],
                Some(CandidateValue::Impossible),
            ),
            // `= 2` and `!= 1` means the value must be 2.
            (
                vec![
                    Operation::NotEquals(local_field.clone(), first_var.clone()),
                    Operation::Equals(local_field.clone(), second_var.clone()),
                ],
                Some(CandidateValue::Single(&variables["second"])),
            ),
            //
            // `one_of [1, 2]` and `!= 1` allows only `2`.
            (
                vec![
                    Operation::OneOf(local_field.clone(), list_var.clone()),
                    Operation::NotEquals(local_field.clone(), first_var.clone()),
                ],
                Some(CandidateValue::Single(&variables["second"])),
            ),
            //
            // `one_of [1, 2, 3]` and `not_one_of [1, 2]` allows only `3`.
            (
                vec![
                    Operation::OneOf(local_field.clone(), longer_list_var.clone()),
                    Operation::NotOneOf(local_field.clone(), list_var.clone()),
                ],
                Some(CandidateValue::Single(&variables["third"])),
            ),
            //
            // `>= 2` and `not_one_of [1, 2]` produces the exclusive > 2 range
            (
                vec![
                    Operation::GreaterThanOrEqual(local_field.clone(), second_var.clone()),
                    Operation::NotOneOf(local_field.clone(), list_var.clone()),
                ],
                Some(CandidateValue::Range(Range::with_start(
                    Bound::Excluded(&variables["second"]),
                    true,
                ))),
            ),
            //
            // `>= 2` and `is_not_null` and `not_one_of [1, 2]` produces the exclusive non-null > 2 range
            (
                vec![
                    Operation::GreaterThanOrEqual(local_field.clone(), second_var.clone()),
                    Operation::NotOneOf(local_field.clone(), list_var.clone()),
                    Operation::IsNotNull(local_field.clone()),
                ],
                Some(CandidateValue::Range(Range::with_start(
                    Bound::Excluded(&variables["second"]),
                    false,
                ))),
            ),
            //
            // `> 2` and `is_not_null` produces the exclusive non-null > 2 range
            (
                vec![
                    Operation::GreaterThan(local_field.clone(), second_var.clone()),
                    Operation::IsNotNull(local_field.clone()),
                ],
                Some(CandidateValue::Range(Range::with_start(
                    Bound::Excluded(&variables["second"]),
                    false,
                ))),
            ),
            //
            // `<= 2` and `!= 2` and `is_not_null` produces the exclusive non-null < 2 range
            (
                vec![
                    Operation::LessThanOrEqual(local_field.clone(), second_var.clone()),
                    Operation::NotEquals(local_field.clone(), second_var.clone()),
                    Operation::IsNotNull(local_field.clone()),
                ],
                Some(CandidateValue::Range(Range::with_end(
                    Bound::Excluded(&variables["second"]),
                    false,
                ))),
            ),
            //
            // `< 2` and `is_not_null` produces the exclusive non-null < 2 range
            (
                vec![
                    Operation::LessThan(local_field.clone(), second_var.clone()),
                    Operation::IsNotNull(local_field.clone()),
                ],
                Some(CandidateValue::Range(Range::with_end(
                    Bound::Excluded(&variables["second"]),
                    false,
                ))),
            ),
            //
            // `is_not_null` by itself only eliminates null
            (
                vec![Operation::IsNotNull(local_field.clone())],
                Some(CandidateValue::Range(Range::full_non_null())),
            ),
            //
            // `!= null` also elminates null
            (
                vec![Operation::NotEquals(local_field.clone(), null_var.clone())],
                Some(CandidateValue::Range(Range::full_non_null())),
            ),
            //
            // `!= 1` by itself doesn't produce any candidates
            (
                vec![Operation::NotEquals(local_field.clone(), first_var.clone())],
                None,
            ),
            //
            // `not_one_of [1, 2]` by itself doesn't produce any candidates
            (
                vec![Operation::NotEquals(local_field.clone(), list_var.clone())],
                None,
            ),
        ];

        for (filters, expected_output) in test_data {
            assert_eq!(
                expected_output,
                compute_statically_known_candidate(&local_field, filters.iter(), &variables),
                "with {filters:?}",
            );
        }

        // Explicitly drop these values, so clippy stops complaining about unneccessary clones earlier.
        drop((
            first_var,
            second_var,
            null_var,
            list_var,
            longer_list_var,
            local_field,
            int_type,
            nullable_int_type,
            list_int_type,
        ));
    }

    #[test]
    fn use_schema_to_exclude_null_from_range() {
        let first: Arc<str> = Arc::from("first");
        let int_type = Type::new("Int!").unwrap();

        let first_var = Argument::Variable(VariableRef {
            variable_name: first.clone(),
            variable_type: int_type.clone(),
        });

        let local_field = LocalField {
            field_name: Arc::from("my_field"),
            field_type: int_type.clone(),
        };

        let variables = btreemap! {
            first => FieldValue::Int64(1),
        };

        let test_data = [
            // The local field is non-nullable.
            // When we apply a range bound on the field, the range must be non-nullable too.
            (
                vec![Operation::GreaterThanOrEqual(
                    local_field.clone(),
                    first_var.clone(),
                )],
                Some(CandidateValue::Range(Range::with_start(
                    Bound::Included(&variables["first"]),
                    false,
                ))),
            ),
            (
                vec![Operation::GreaterThan(
                    local_field.clone(),
                    first_var.clone(),
                )],
                Some(CandidateValue::Range(Range::with_start(
                    Bound::Excluded(&variables["first"]),
                    false,
                ))),
            ),
            (
                vec![Operation::LessThan(local_field.clone(), first_var.clone())],
                Some(CandidateValue::Range(Range::with_end(
                    Bound::Excluded(&variables["first"]),
                    false,
                ))),
            ),
            (
                vec![Operation::LessThanOrEqual(
                    local_field.clone(),
                    first_var.clone(),
                )],
                Some(CandidateValue::Range(Range::with_end(
                    Bound::Included(&variables["first"]),
                    false,
                ))),
            ),
        ];

        for (filters, expected_output) in test_data {
            assert_eq!(
                expected_output,
                compute_statically_known_candidate(&local_field, filters.iter(), &variables),
                "with {filters:?}",
            );
        }

        // Explicitly drop these values, so clippy stops complaining about unneccessary clones earlier.
        drop((first_var, local_field, int_type));
    }
}