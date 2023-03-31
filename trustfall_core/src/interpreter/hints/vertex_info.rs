use std::{collections::BTreeMap, ops::Bound, sync::Arc};

use itertools::Itertools;

use crate::ir::{Argument, FieldValue, IREdge, IRFold, IRQueryComponent, IRVertex, Operation, Vid};

use super::{CandidateValue, EdgeInfo, Range};

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
        let current_vertex = self.current_vertex();
        let query_variables = self.query_variables();

        let filters_on_this_property: Vec<_> = current_vertex
            .filters
            .iter()
            .filter(|op| op.left().field_name.as_ref() == property)
            .collect();
        if filters_on_this_property.is_empty() {
            return None;
        }

        let (candidate_filters, post_processing_filters): (Vec<_>, Vec<&Operation<_, _>>) =
            filters_on_this_property
                .iter()
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
                    _ => itertools::Either::Right(op),
                });

        let mut candidate = if candidate_filters.is_empty() {
            // No valid candidate-producing filters found.
            return None;
        } else {
            candidate_filters
                .into_iter()
                .fold(CandidateValue::All, |mut acc, e| {
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
        let disallowed_values: Vec<_> = filters_and_values
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
            .flatten()
            .collect();
        match &mut candidate {
            CandidateValue::Single(s) => {
                if disallowed_values.contains(s) {
                    candidate = CandidateValue::Impossible;
                }
            }
            CandidateValue::Multiple(vals) => {
                vals.retain(|v| !disallowed_values.contains(v));
            }
            CandidateValue::Range(range) => {
                // TODO: take advantage of discrete inclusive/exclusive values
                //       for types like integers
                if let Bound::Included(value) = range.start_bound() {
                    if disallowed_values.contains(value) {
                        range.intersect(Range::with_start(Bound::Excluded(*value), true));
                    }
                }
                if let Bound::Included(value) = range.end_bound() {
                    if disallowed_values.contains(value) {
                        range.intersect(Range::with_end(Bound::Excluded(*value), true));
                    }
                }
            }
            _ => {}
        };
        candidate.normalize();

        // TODO: use the other kinds of filters to exclude more candidate values

        Some(candidate)
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
