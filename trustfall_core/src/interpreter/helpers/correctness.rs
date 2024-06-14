use std::{collections::BTreeMap, fmt::Debug, num::NonZeroUsize, sync::Arc};

use crate::{
    interpreter::{Adapter, DataContext, InterpretedQuery, ResolveEdgeInfo, ResolveInfo},
    ir::{
        ContextField, EdgeParameters, Eid, FieldRef, FieldValue, IREdge, IRQuery, IRQueryComponent,
        IRVertex, TransparentValue, Type, Vid,
    },
    schema::{Schema, SchemaAdapter},
    TryIntoStruct,
};

/// Run a series of "dry run" checks to ensure an adapter is properly implemented.
///
/// Checks the following invariants about adapters:
/// - No panics when calling [`resolve_property()`][prop] for all properties in the schema,
///   as well as for the special `__typename` property that implicitly exists on all types.
/// - No panics when calling [`resolve_neighbors()`][neigh] edges in the schema that
///   do not have required non-nullable edge parameters.
/// - No panics when calling [`resolve_coercion()`][coerc] for all interface implementations
///   in the schema.
/// - [`resolve_property()`][prop] produces a property value of [`FieldValue::Null`]
///   for each context with a `None` active vertex.
/// - [`resolve_neighbors()`][neigh] produces an empty iterator of neighboring vertices
///   for each context with a `None` active vertex.
/// - [`resolve_coercion()`][coerc] produces a `false` coercion outcome
///   for each context with a `None` active vertex.
/// - [`resolve_property()`][prop], [`resolve_neighbors()`][neigh],
///   and [`resolve_coercion()`][coerc] yield contexts in the same relative order they received,
///   without any reordering compared to the input order.
///
/// Failure to uphold any of the above invariants will cause a panic.
///
/// # Example
///
/// This function would normally be used in a test case so that broken adapter invariants
/// cause test failures:
/// ```rust
/// # use trustfall_core::{
/// #    interpreter::helpers::check_adapter_invariants,
/// #    schema::{Schema, SchemaAdapter},
/// # };
/// #
/// # fn get_schema_and_adapter() -> (Schema, SchemaAdapter<'static>) {
/// #     let schema = Schema::parse(SchemaAdapter::schema_text()).expect("not a valid schema");
/// #     let adapter = SchemaAdapter::new(Box::leak(Box::new(schema.clone())));
/// #     (schema, adapter)
/// # }
/// #
/// #[test]
/// fn ensure_adapter_satisfies_invariants() {
///     let (schema, adapter) = get_schema_and_adapter();
///     check_adapter_invariants(&schema, adapter);
/// }
/// ```
///
/// # Limitations
///
/// Parameterized edges are checked using the default values of all their parameters.
/// Nullable parameters are implicitly considered to have `null` as a default value.
///
/// Edges that take any non-nullable parameters without specified default values are not checked.
///
/// # "My adapter fails this test, now what?"
///
/// Effectively all valid Trustfall adapters should uphold these invariants and pass these checks.
/// In _extremely rare_ cases, it's possible that an adapter might work properly even if
/// it cannot pass the checks in this function. However, doing so exposes those implementations
/// to compatibility and correctness risks, and may cause difficult-to-debug bugs.
///
/// As a best practice, [`Adapter`] implementations should pass this function.
///
/// [prop]: Adapter::resolve_property
/// [neigh]: Adapter::resolve_neighbors
/// [coerc]: Adapter::resolve_coercion
pub fn check_adapter_invariants<'a, A: Adapter<'a>>(schema: &Schema, adapter: A) {
    let schema_adapter = Arc::new(SchemaAdapter::new(schema));
    let meta_schema = Schema::parse(SchemaAdapter::schema_text()).expect("invalid schema");

    // How many contexts to use when testing the non-reordering invariant
    // across the adapter methods. Too high would make the test too slow, while
    // too low might make us lose sensitivity since reordering is more likely to manifest
    // when there are more opportunities for it to happen.
    let sample_size = 8;

    check_properties_are_implemented(&meta_schema, schema_adapter.clone(), &adapter, sample_size);
    check_edges_are_implemented(&meta_schema, schema_adapter.clone(), &adapter, sample_size);
    check_type_coercions_are_implemented(&meta_schema, schema_adapter, &adapter, sample_size);
}

fn make_contexts<V: Debug + Clone>(count: usize) -> Vec<DataContext<V>> {
    let mut result = Vec::with_capacity(count);
    for i in 0..=count {
        let mut ctx = DataContext::new(None);
        ctx.values.push(FieldValue::Int64(i as i64));
        result.push(ctx);
    }
    result
}

fn get_context_order_values<V: Debug + Clone>(ctxs: &[DataContext<V>]) -> Vec<i64> {
    ctxs.iter()
        .map(|ctx| {
            let order = ctx
                .values
                .last()
                .expect("no ordering value pushed")
                .as_i64()
                .expect("ordering value was not a number");
            assert_eq!(
                1,
                ctx.values.len(),
                "more than one value in the values stack: {:?}",
                ctx.values
            );
            order
        })
        .collect()
}

fn run_query<'a, A: Adapter<'a> + 'a, T: serde::de::DeserializeOwned>(
    schema: &Schema,
    adapter: Arc<A>,
    query: &str,
    variables: BTreeMap<Arc<str>, FieldValue>,
) -> impl Iterator<Item = T> + 'a {
    let indexed = crate::frontend::parse(schema, query).expect("not a valid query");
    crate::interpreter::execution::interpret_ir(adapter, indexed, Arc::new(variables))
        .expect("execution error")
        .map(|row| row.try_into_struct::<T>().expect("incorrect result shape"))
}

/// Construct a *believable* [`ResolveInfo`] that would pass muster under a cursory examination.
///
/// Similar to [`make_resolve_edge_info_for_edge_check`], but for properties.
///
/// The query behind this [`ResolveInfo`] may not be valid, and we are not aiming to generate
/// a 100% valid query since that would be impractically complex. Instead, we're aiming
/// to produce a believable value that would work fine for completeness-checking of
/// most adapters — especially adapters built with `trustfall_stubgen`.
///
/// The [`ResolveInfo`] we construct here will claim to have "no information" whenever possible.
/// Proving its answers implausible given a schema would take some work: for example, the adapter
/// would have to inspect the schema and show that there exists a type that cannot be reached
/// via any query entrypoint since no starting edge points to it, so its vertex
/// couldn't possibly have been `Vid(1)` in the query.
fn make_resolve_info_for_property_check(
    type_name: &Arc<str>,
    property_name: &Arc<str>,
    property_type: &str,
) -> ResolveInfo {
    let vid = Vid::new(NonZeroUsize::new(1).expect("invalid Vid"));
    let query = IRQuery {
        root_name: type_name.clone(),
        root_parameters: EdgeParameters::new(Arc::new(Default::default())),
        root_component: Arc::new(IRQueryComponent {
            root: vid,
            vertices: btreemap! {
                vid => IRVertex {
                    vid,
                    type_name: type_name.clone(),
                    coerced_from_type: None,
                    filters: vec![],
                },
            },
            edges: Default::default(),
            folds: Default::default(),
            outputs: btreemap! {
                property_name.clone() => FieldRef::ContextField(ContextField {
                    vertex_id: vid,
                    field_name: property_name.clone(),
                    field_type: Type::parse(property_type).expect("not a valid type"),
                })
            },
        }),
        variables: Default::default(),
    };
    let query = InterpretedQuery::from_query_and_arguments(
        Arc::new(query.try_into().expect("not a valid query")),
        Arc::new(BTreeMap::new()),
    )
    .expect("not a valid query");
    ResolveInfo::new(query, vid, true)
}

fn check_properties_are_implemented<'a, A: Adapter<'a>>(
    meta_schema: &Schema,
    schema_adapter: Arc<SchemaAdapter<'_>>,
    adapter_under_test: &A,
    sample_size: usize,
) {
    let initial_contexts = make_contexts::<A::Vertex>(sample_size);
    let initial_context_order: Vec<_> = get_context_order_values(&initial_contexts);

    let query = r#"
{
    VertexType {
        type_name: name @output

        property @fold {
            property_names: name @output
            property_types: type @output
        }
    }
}
"#;
    let variables: BTreeMap<Arc<str>, FieldValue> = Default::default();

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct Output {
        type_name: Arc<str>,
        property_names: Vec<Arc<str>>,
        property_types: Vec<Arc<str>>,
    }

    let typename_property: Arc<str> = Arc::from("__typename");
    let typename_type: Arc<str> = Arc::from("String!");

    for output in
        run_query::<SchemaAdapter<'_>, Output>(meta_schema, schema_adapter, query, variables)
    {
        let type_name = &output.type_name;
        let property_data = output
            .property_names
            .into_iter()
            .zip(output.property_types)
            .chain([(typename_property.clone(), typename_type.clone())]);

        for (property_name, property_type) in property_data {
            let contexts = Box::new(initial_contexts.clone().into_iter());
            let resolve_info = make_resolve_info_for_property_check(
                &output.type_name,
                &property_name,
                &property_type,
            );

            let mut final_contexts = Vec::with_capacity(sample_size);

            for (ctx, value) in adapter_under_test.resolve_property(
                contexts,
                type_name,
                &property_name,
                &resolve_info,
            ) {
                assert_eq!(
                    FieldValue::NULL,
                    value,
                    "resolve_property() unexpectedly produced {value:?} instead \
                    of `FieldValue::Null` for a vertex that didn't exist with \
                    type name '{type_name}' property '{property_name}'",
                );
                final_contexts.push(ctx);
            }
            assert_eq!(
                initial_contexts.len(),
                final_contexts.len(),
                "adapter lost {} contexts inside resolve_property() \
                for type name '{type_name}' and property '{property_name}'",
                initial_contexts.len() - final_contexts.len(),
            );
            let final_context_order: Vec<_> = get_context_order_values(&final_contexts);
            assert_eq!(
                initial_context_order, final_context_order,
                "adapter illegally reordered contexts inside resolve_property() \
                for type name '{type_name}' and property '{property_name}'"
            )
        }
    }
}

/// Construct a *believable* [`ResolveEdgeInfo`] that would pass muster under a cursory examination.
///
/// Similar to [`make_resolve_info_for_property_check`], but for edges.
///
/// The query behind this [`ResolveEdgeInfo`] may not be valid, and we are not aiming to generate
/// a 100% valid query since that would be impractically complex. Instead, we're aiming
/// to produce a believable value that would work fine for completeness-checking of
/// most adapters — especially adapters built with `trustfall_stubgen`.
///
/// The [`ResolveEdgeInfo`] we construct here will claim to have "no information" whenever possible.
/// Proving its answers implausible given a schema would take some work: for example, the adapter
/// would have to inspect the schema and show that there exists a type that cannot be reached
/// via any query entrypoint since no starting edge points to it, so its vertex
/// couldn't possibly have been `Vid(1)` in the query.
fn make_resolve_edge_info_for_edge_check(
    type_name: &Arc<str>,
    edge_name: &Arc<str>,
    parameters: &EdgeParameters,
    target_type: &Arc<str>,
) -> crate::interpreter::ResolveEdgeInfo {
    let vid = Vid::new(NonZeroUsize::new(1).expect("invalid Vid"));
    let to_vid = Vid::new(NonZeroUsize::new(2).expect("invalid Vid"));
    let eid = Eid::new(NonZeroUsize::new(1).expect("invalid Eid"));
    let property_name: Arc<str> = Arc::from("__typename");
    let query = IRQuery {
        root_name: type_name.clone(),
        root_parameters: EdgeParameters::new(Arc::new(Default::default())),
        root_component: Arc::new(IRQueryComponent {
            root: vid,
            vertices: btreemap! {
                vid => IRVertex {
                    vid,
                    type_name: type_name.clone(),
                    coerced_from_type: None,
                    filters: vec![],
                },
                to_vid => IRVertex {
                    vid: to_vid,
                    type_name: target_type.clone(),
                    coerced_from_type: None,
                    filters: vec![],
                }
            },
            edges: btreemap! {
                eid => Arc::new(IREdge {
                    eid,
                    from_vid: vid,
                    to_vid,
                    edge_name: edge_name.clone(),
                    parameters: parameters.clone(),
                    optional: false,
                    recursive: None,
                }),
            },
            folds: Default::default(),
            outputs: btreemap! {
                property_name.clone() => FieldRef::ContextField(ContextField {
                    vertex_id: vid,
                    field_name: property_name,
                    field_type: Type::parse("String!").expect("not a valid type"),
                })
            },
        }),
        variables: Default::default(),
    };
    let query = InterpretedQuery::from_query_and_arguments(
        Arc::new(query.try_into().expect("not a valid query")),
        Arc::new(BTreeMap::new()),
    )
    .expect("not a valid query");
    ResolveEdgeInfo::new(query, vid, to_vid, eid)
}

fn check_edges_are_implemented<'a, A: Adapter<'a>>(
    meta_schema: &Schema,
    schema_adapter: Arc<SchemaAdapter<'_>>,
    adapter_under_test: &A,
    sample_size: usize,
) {
    let initial_contexts = make_contexts::<A::Vertex>(sample_size);
    let initial_context_order: Vec<_> = get_context_order_values(&initial_contexts);

    let query = r#"
{
    VertexType {
        type_name: name @output

        edge {
            edge_name: name @output

            parameter_: parameter @fold {
                name @output
                type @output
                default @output
            }

            target {
                target_type: name @output
            }
        }
    }
}
"#;
    let variables: BTreeMap<Arc<str>, FieldValue> = Default::default();

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct Output {
        type_name: Arc<str>,
        edge_name: Arc<str>,
        parameter_name: Vec<Arc<str>>,
        parameter_type: Vec<Arc<str>>,
        parameter_default: Vec<Option<String>>,
        target_type: Arc<str>,
    }

    for output in
        run_query::<SchemaAdapter<'_>, Output>(meta_schema, schema_adapter, query, variables)
    {
        let type_name = output.type_name;
        let edge_name = output.edge_name;

        let parameter_defaults: Vec<_> = output
            .parameter_default
            .into_iter()
            .map(|value| {
                value.map(|content| {
                    let transparent_value: TransparentValue =
                        serde_json::from_str(&content).expect("invalid serialized content");
                    FieldValue::from(transparent_value)
                })
            })
            .collect();
        if parameter_defaults.contains(&Option::None) {
            // This edge has a parameter without a default value.
            // We can't check it since we don't know what values are valid to provide.
            continue;
        }

        let edge_parameters: Arc<BTreeMap<Arc<str>, FieldValue>> = Arc::new(
            output
                .parameter_name
                .into_iter()
                .zip(parameter_defaults)
                .map(|(name, value)| (name, value.expect("parameter has a default")))
                .collect(),
        );
        let parameters = EdgeParameters::new(edge_parameters.clone());

        let contexts = Box::new(initial_contexts.clone().into_iter());
        let resolve_info = make_resolve_edge_info_for_edge_check(
            &type_name,
            &edge_name,
            &parameters,
            &output.target_type,
        );

        let mut final_contexts = Vec::with_capacity(sample_size);

        for (ctx, mut neighbors) in adapter_under_test.resolve_neighbors(
            contexts,
            &type_name,
            &edge_name,
            &parameters,
            &resolve_info,
        ) {
            assert!(
                neighbors.next().is_none(),
                "resolve_neighbors() produced a non-empty neighbor iterator \
                for a vertex that didn't exist: \
                type '{type_name}' edge '{edge_name}' parameters {edge_parameters:?}"
            );
            final_contexts.push(ctx);
        }
        assert_eq!(
            initial_contexts.len(),
            final_contexts.len(),
            "adapter lost {} contexts inside resolve_neighbors() \
            for type '{type_name}' edge '{edge_name}' with parameters {edge_parameters:?}",
            initial_contexts.len() - final_contexts.len(),
        );
        let final_context_order: Vec<_> = get_context_order_values(&final_contexts);
        assert_eq!(
            initial_context_order, final_context_order,
            "adapter illegally reordered contexts inside resolve_neighbors() \
            for type '{type_name}' edge '{edge_name}' with parameters {edge_parameters:?}"
        )
    }
}

/// Construct a *believable* [`ResolveInfo`] that would pass muster under a cursory examination.
///
/// Similar to [`make_resolve_info_for_property_check`], but for type coercions.
///
/// The query behind this [`ResolveInfo`] may not be valid, and we are not aiming to generate
/// a 100% valid query since that would be impractically complex. Instead, we're aiming
/// to produce a believable value that would work fine for completeness-checking of
/// most adapters — especially adapters built with `trustfall_stubgen`.
///
/// The [`ResolveInfo`] we construct here will claim to have "no information" whenever possible.
/// Proving its answers implausible given a schema would take some work: for example, the adapter
/// would have to inspect the schema and show that there exists a type that cannot be reached
/// via any query entrypoint since no starting edge points to it, so its vertex
/// couldn't possibly have been `Vid(1)` in the query.
fn make_resolve_info_for_type_coercion(
    type_name: &Arc<str>,
    coerce_to: &Arc<str>,
    typename_property: &Arc<str>,
) -> ResolveInfo {
    let vid = Vid::new(NonZeroUsize::new(1).expect("invalid Vid"));
    let query = IRQuery {
        root_name: type_name.clone(),
        root_parameters: EdgeParameters::new(Arc::new(Default::default())),
        root_component: Arc::new(IRQueryComponent {
            root: vid,
            vertices: btreemap! {
                vid => IRVertex {
                    vid,
                    type_name: coerce_to.clone(),
                    coerced_from_type: Some(type_name.clone()),
                    filters: vec![],
                },
            },
            edges: Default::default(),
            folds: Default::default(),
            outputs: btreemap! {
                typename_property.clone() => FieldRef::ContextField(ContextField {
                    vertex_id: vid,
                    field_name: typename_property.clone(),
                    field_type: Type::parse("String!").expect("not a valid type"),
                })
            },
        }),
        variables: Default::default(),
    };
    let query = InterpretedQuery::from_query_and_arguments(
        Arc::new(query.try_into().expect("not a valid query")),
        Arc::new(BTreeMap::new()),
    )
    .expect("not a valid query");
    ResolveInfo::new(query, vid, true)
}

fn check_type_coercions_are_implemented<'a, A: Adapter<'a>>(
    meta_schema: &Schema,
    schema_adapter: Arc<SchemaAdapter<'_>>,
    adapter_under_test: &A,
    sample_size: usize,
) {
    let initial_contexts = make_contexts::<A::Vertex>(sample_size);
    let initial_context_order: Vec<_> = get_context_order_values(&initial_contexts);

    let query = r#"
{
    VertexType {
        coerce_to: name @output

        implements {
            type_name: name @output
        }
    }
}
"#;
    let variables: BTreeMap<Arc<str>, FieldValue> = Default::default();

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct Output {
        type_name: Arc<str>,
        coerce_to: Arc<str>,
    }

    let typename_property: Arc<str> = Arc::from("__typename");

    for output in
        run_query::<SchemaAdapter<'_>, Output>(meta_schema, schema_adapter, query, variables)
    {
        let type_name = &output.type_name;
        let coerce_to = &output.coerce_to;

        let contexts = Box::new(initial_contexts.clone().into_iter());
        let resolve_info =
            make_resolve_info_for_type_coercion(type_name, coerce_to, &typename_property);

        let mut final_contexts = Vec::with_capacity(sample_size);

        for (ctx, value) in
            adapter_under_test.resolve_coercion(contexts, type_name, coerce_to, &resolve_info)
        {
            assert!(
                !value,
                "resolve_coercion() claimed that a non-existent vertex could be coerced \
                from type {type_name} to {coerce_to}",
            );
            final_contexts.push(ctx);
        }
        assert_eq!(
            initial_contexts.len(),
            final_contexts.len(),
            "adapter lost {} contexts inside resolve_coercion() \
            for type_name '{type_name}' and coerce_to_type '{coerce_to}'",
            initial_contexts.len() - final_contexts.len(),
        );
        let final_context_order: Vec<_> = get_context_order_values(&final_contexts);
        assert_eq!(
            initial_context_order, final_context_order,
            "adapter illegally reordered contexts inside resolve_coercion() \
            for type_name '{type_name}' and coerce_to_type '{coerce_to}'",
        )
    }
}
