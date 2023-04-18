use std::{
    collections::BTreeMap,
    ops::{Bound, RangeBounds},
    sync::Arc,
};

use crate::{
    interpreter::InterpretedQuery,
    ir::{
        Argument, FieldRef, FieldValue, IREdge, IRFold, IRQueryComponent, IRVertex, LocalField,
        Operation, Vid,
    },
};

use super::{dynamic::DynamicallyResolvedValue, CandidateValue, EdgeInfo, Range};

/// Information about what the currently-executing query needs at a specific vertex.
#[cfg_attr(docsrs, doc(notable_trait))]
pub trait VertexInfo: super::sealed::__Sealed {
    /// The unique ID of the vertex this [`VertexInfo`] describes.
    fn vid(&self) -> Vid;

    /// The type coercion (`... on SomeType`) applied by the query at this vertex, if any.
    fn coerced_to_type(&self) -> Option<&Arc<str>>;

    /// Check whether the query demands this vertex property to have specific values:
    /// a single value, or one of a set or range of values. The candidate values
    /// are known *statically*: up-front, without executing any of the query.
    ///
    /// For example, filtering a property based on a query variable (e.g.
    /// `@filter(op: "=", value: ["$expected"])`) means the filtered property will
    /// need to match the value of the `expected` query variable. This variable's value is known
    /// up-front at the beginning of query execution, so the filtered property has
    /// a statically-required value.
    ///
    /// In contrast, filters relying the value of a `@tag` do not produce
    /// statically-required values, since the `@tag` value must be computed at runtime.
    /// For this case, see the [`VertexInfo::dynamically_required_property()`] method.
    fn statically_required_property(&self, name: &str) -> Option<CandidateValue<&FieldValue>>;

    /// Check whether the query demands this vertex property to have specific values:
    /// a single value, or one of a set or range of values. The candidate values
    /// are only known *dynamically* i.e. require some of the query
    /// to have already been executed at the point when this method is called.
    ///
    /// For example, filtering a property with `@filter(op: "=", value: ["%expected"])`
    /// means the property must have a value equal to the value of an earlier property
    /// whose value is tagged like `@tag(name: "expected")`. If the vertex containing
    /// the tagged property has already been resolved in this query, this method will offer
    /// to produce candidate values based on that tag's value.
    ///
    /// If *only* static information and no dynamic information is known about a property's value,
    /// this method will return `None` in order to avoid unnecessary cloning.
    /// The [`VertexInfo::statically_required_property()`] method can be used to retrieve
    /// the statically-known information about the property's value.
    ///
    /// If *both* static and dynamic information is known about a property's value, all information
    /// will be merged automatically and presented via the output of this method.
    fn dynamically_required_property(&self, name: &str) -> Option<DynamicallyResolvedValue>;

    /// Returns info for the first not-yet-resolved edge by the given name that is *mandatory*:
    /// this vertex must contain the edge, or its result set will be discarded.
    ///
    /// Edges marked `@optional`, `@fold`, or `@recurse` are not mandatory:
    /// - `@optional` edges that don't exist produce `null` outputs.
    /// - `@fold` edges that don't exist produce empty aggregations.
    /// - `@recurse` always starts at depth 0 (i.e. returning the *current* vertex),
    ///   so the edge is not required to exist.
    fn first_mandatory_edge(&self, name: &str) -> Option<EdgeInfo>;

    /// Returns info for the first not-yet-resolved edge by the given name.
    ///
    /// Just a convenience wrapper over [`VertexInfo::edges_with_name()`].
    fn first_edge(&self, name: &str) -> Option<EdgeInfo>;

    /// Returns an iterator of all not-yet-resolved edges by that name originating from this vertex.
    ///
    /// This is the building block of [`VertexInfo::first_edge()`].
    /// When possible, prefer using that method as it will lead to more readable code.
    fn edges_with_name<'a>(&'a self, name: &'a str) -> Box<dyn Iterator<Item = EdgeInfo> + 'a>;

    /// Returns an iterator of all not-yet-resolved edges by that name that are *mandatory*:
    /// this vertex must contain the edge, or its result set will be discarded.
    ///
    /// This is the building block of [`VertexInfo::first_mandatory_edge()`].
    /// When possible, prefer using that method as it will lead to more readable code.
    fn mandatory_edges_with_name<'a>(
        &'a self,
        name: &'a str,
    ) -> Box<dyn Iterator<Item = EdgeInfo> + 'a>;
}

pub(super) trait InternalVertexInfo: super::sealed::__Sealed {
    fn query(&self) -> &InterpretedQuery;

    fn non_binding_filters(&self) -> bool;

    /// How far query execution has progressed thus far:
    /// - `Bound::Included` means that data from that [`Vid`] is available, and
    /// - `Bound::Excluded` means that data from that [`Vid`] is not yet available.
    /// Data from vertices with [`Vid`] values smaller than the given number is always available.
    fn execution_frontier(&self) -> Bound<Vid>;

    /// The vertex that this [`InternalVertexInfo`] represents.
    fn current_vertex(&self) -> &IRVertex;

    /// The component where the vertex represented by this [`InternalVertexInfo`] is found.
    fn current_component(&self) -> &IRQueryComponent;

    /// The component where resolution is happening,
    /// i.e. where the traversal through the optimization hints began.
    fn starting_component(&self) -> &IRQueryComponent;

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

    fn statically_required_property(&self, property: &str) -> Option<CandidateValue<&FieldValue>> {
        if self.non_binding_filters() {
            // This `VertexInfo` is in a place where the filters applied to fields
            // don't actually constrain their value in the usual way that lends itself
            // to optimization.
            //
            // For example, we may be looking at the data of a vertex produced by a `@recurse`,
            // where the *final* vertices produced by the recursion must satisfy the filters, but
            // intermediate layers of the recursion do not: non-matching ones will get filtered out,
            // but only after the edge recurses to their own neighbors as well.
            return None;
        }

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

    fn dynamically_required_property(&self, property: &str) -> Option<DynamicallyResolvedValue> {
        if self.non_binding_filters() {
            // This `VertexInfo` is in a place where the filters applied to fields
            // don't actually constrain their value in the usual way that lends itself
            // to optimization.
            //
            // For example, we may be looking at the data of a vertex produced by a `@recurse`,
            // where the *final* vertices produced by the recursion must satisfy the filters, but
            // intermediate layers of the recursion do not: non-matching ones will get filtered out,
            // but only after the edge recurses to their own neighbors as well.
            return None;
        }

        // We only care about filtering operations that are all of the following:
        // - on the requested property of this vertex;
        // - dynamically-resolvable, i.e. depend on tagged arguments,
        // - the used tagged argument is from a vertex that has already been computed
        //   at the time this call was made, and
        // - use a supported filtering operation using those tagged arguments.
        let resolved_range = (Bound::Unbounded, self.execution_frontier());
        let relevant_filters: Vec<_> = filters_on_local_property(self.current_vertex(), property)
            .filter(|op| {
                matches!(
                    op,
                    Operation::Equals(..)
                        | Operation::NotEquals(..)
                        | Operation::LessThan(..)
                        | Operation::LessThanOrEqual(..)
                        | Operation::GreaterThan(..)
                        | Operation::GreaterThanOrEqual(..)
                        | Operation::OneOf(..)
                ) && match op.right() {
                    Some(Argument::Tag(FieldRef::ContextField(ctx))) => {
                        // Ensure the vertex holding the @tag has already been computed.
                        resolved_range.contains(&ctx.vertex_id)
                    }
                    Some(Argument::Tag(FieldRef::FoldSpecificField(fsf))) => {
                        // Ensure the fold holding the @tag has already been computed.
                        resolved_range.contains(&fsf.fold_root_vid)
                    }
                    _ => false,
                }
            })
            .collect();

        // Early-return in case there are no filters that apply here.
        let first_filter = relevant_filters.first()?;

        let initial_candidate = self
            .statically_required_property(property)
            .unwrap_or_else(|| {
                if first_filter.left().field_type.nullable {
                    CandidateValue::All
                } else {
                    CandidateValue::Range(Range::full_non_null())
                }
            })
            .cloned();

        // Right now, this API only supports materializing the constraint from a single tag.
        // Choose which @filter to choose as the one providing the value.
        //
        // In order of priority, we'll choose:
        // - an `=` filter
        // - a `one_of` filter
        // - a `< / <= / > / >=` filter
        // - a `!=` filter,
        // breaking ties based on which filter was specified first.
        let filter_to_use = {
            relevant_filters
                .iter()
                .find(|op| matches!(op, Operation::Equals(..)))
                .unwrap_or_else(|| {
                    relevant_filters
                        .iter()
                        .find(|op| matches!(op, Operation::OneOf(..)))
                        .unwrap_or_else(|| {
                            relevant_filters
                                .iter()
                                .find(|op| {
                                    matches!(
                                        op,
                                        Operation::LessThan(..)
                                            | Operation::LessThanOrEqual(..)
                                            | Operation::GreaterThan(..)
                                            | Operation::GreaterThanOrEqual(..)
                                    )
                                })
                                .unwrap_or(first_filter)
                        })
                })
        };

        let field = filter_to_use
            .right()
            .expect("filter did not have an operand")
            .as_tag()
            .expect("operand was not a tag");
        let bare_operation = filter_to_use
            .try_map(|_| Ok::<(), ()>(()), |_| Ok(()))
            .expect("removing operands failed");
        Some(DynamicallyResolvedValue::new(
            self.query().clone(),
            self.starting_component(),
            field,
            bare_operation,
            initial_candidate,
        ))
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

    fn mandatory_edges_with_name<'a>(
        &'a self,
        name: &'a str,
    ) -> Box<dyn Iterator<Item = EdgeInfo> + 'a> {
        if self.non_binding_filters() {
            Box::new(std::iter::empty())
        } else {
            Box::new(
                self.edges_with_name(name)
                    .filter(|edge| !edge.folded && !edge.optional && edge.recursive.is_none()),
            )
        }
    }

    fn first_mandatory_edge(&self, name: &str) -> Option<EdgeInfo> {
        self.mandatory_edges_with_name(name).next()
    }

    fn first_edge(&self, name: &str) -> Option<EdgeInfo> {
        self.edges_with_name(name).next()
    }
}

fn filters_on_local_property<'a: 'b, 'b>(
    vertex: &'a IRVertex,
    property_name: &'b str,
) -> impl Iterator<Item = &'a Operation<LocalField, Argument>> + 'b {
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
    let is_subject_field_nullable = field.field_type.nullable;
    super::filters::candidate_from_statically_evaluated_filters(
        relevant_filters,
        query_variables,
        is_subject_field_nullable,
    )
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
