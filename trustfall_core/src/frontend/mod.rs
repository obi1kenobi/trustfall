//! Frontend for Trustfall: takes a parsed query, validates it, and turns it into IR.
#![allow(dead_code, unused_variables, unused_mut)]
use std::{collections::BTreeMap, fmt::Write, iter::successors, num::NonZeroUsize, sync::Arc};

use async_graphql_parser::{
    types::{ExecutableDocument, FieldDefinition, TypeDefinition, TypeKind},
    Positioned,
};
use error::TransformTypeError;
use filters::make_filter_expr;
use smallvec::SmallVec;

use crate::{
    graphql_query::{
        directives::{
            FilterDirective, FoldGroup, OperatorArgument, RecurseDirective, TransformDirective,
            TransformOp,
        },
        query::{parse_document, FieldConnection, FieldNode, Query},
    },
    ir::{
        get_typename_meta_field, Argument, ContextField, EdgeParameters, Eid, FieldRef, FieldValue,
        FoldSpecificField, FoldSpecificFieldKind, IREdge, IRFold, IRQuery, IRQueryComponent,
        IRVertex, IndexedQuery, LocalField, Operation, OperationSubject, Recursive, Tid, Transform,
        TransformBase, TransformedField, TransformedValue, Type, Vid, TYPENAME_META_FIELD,
    },
    schema::{get_builtin_scalars, FieldOrigin, Schema},
    util::{BTreeMapTryInsertExt, TryCollectUniqueKey},
};

use self::{
    error::{DuplicatedNamesConflict, FilterTypeError, FrontendError, ValidationError},
    outputs::OutputHandler,
    tags::TagHandler,
    util::{get_underlying_named_type, ComponentPath},
    validation::validate_query_against_schema,
};

pub mod error;
mod filters;
mod outputs;
mod tags;
mod util;
mod validation;

/// Parses a query string to the Trustfall IR using a provided
/// [Schema]. May fail if [parse_to_ir] fails for the provided schema and query.
pub fn parse(schema: &Schema, query: impl AsRef<str>) -> Result<Arc<IndexedQuery>, FrontendError> {
    let ir_query = parse_to_ir(schema, query)?;

    // .unwrap() must be safe here, since freshly-generated IRQuery objects must always
    // be safe to convert to IndexedQuery. This is a try_into() instead of into() because
    // IRQuery is Serialize/Deserialize and may therefore have been edited (e.g. by hand)
    // before being converted into IndexedQuery.
    let indexed_query: IndexedQuery = ir_query.try_into().unwrap();

    Ok(Arc::from(indexed_query))
}

/// Parses a query string to IR using a [Schema].
pub fn parse_to_ir<T: AsRef<str>>(schema: &Schema, query: T) -> Result<IRQuery, FrontendError> {
    let document = async_graphql_parser::parse_query(query)?;
    let q = parse_document(&document)?;
    make_ir_for_query(schema, &q)
}

pub fn parse_doc(schema: &Schema, document: &ExecutableDocument) -> Result<IRQuery, FrontendError> {
    let q = parse_document(document)?;
    make_ir_for_query(schema, &q)
}

fn get_field_name_and_type_from_schema<'a>(
    defined_fields: &'a [Positioned<FieldDefinition>],
    field_node: &FieldNode,
) -> (&'a str, Arc<str>, Arc<str>, Type) {
    if field_node.name.as_ref() == TYPENAME_META_FIELD {
        let field_name = get_typename_meta_field();
        return (
            TYPENAME_META_FIELD,
            field_name.clone(),
            field_name.clone(),
            Type::new_named_type("String", false),
        );
    }

    for defined_field in defined_fields {
        let field_name = &defined_field.node.name.node;
        let field_raw_type = &defined_field.node.ty.node;
        if field_name.as_ref() == field_node.name.as_ref() {
            let pre_coercion_type_name: Arc<str> =
                get_underlying_named_type(field_raw_type).to_string().into();
            let post_coercion_type_name = if let Some(coerced_to) = &field_node.coerced_to {
                coerced_to.clone()
            } else {
                pre_coercion_type_name.clone()
            };
            return (
                field_name,
                pre_coercion_type_name,
                post_coercion_type_name,
                Type::from_type(field_raw_type),
            );
        }
    }

    unreachable!()
}

fn get_vertex_type_definition_from_schema<'a>(
    schema: &'a Schema,
    vertex_type_name: &str,
) -> Result<&'a TypeDefinition, FrontendError> {
    schema.vertex_types.get(vertex_type_name).ok_or_else(|| {
        FrontendError::ValidationError(ValidationError::NonExistentType(
            vertex_type_name.to_owned(),
        ))
    })
}

fn get_edge_definition_from_schema<'a>(
    schema: &'a Schema,
    type_name: &str,
    edge_name: &str,
) -> &'a FieldDefinition {
    let defined_fields = get_vertex_field_definitions(schema, type_name);

    for defined_field in defined_fields {
        let field_name = defined_field.node.name.node.as_str();
        if field_name == edge_name {
            return &defined_field.node;
        }
    }

    unreachable!()
}

fn get_vertex_field_definitions<'a>(
    schema: &'a Schema,
    type_name: &str,
) -> &'a Vec<Positioned<FieldDefinition>> {
    match &schema.vertex_types[type_name].kind {
        TypeKind::Object(o) => &o.fields,
        TypeKind::Interface(i) => &i.fields,
        _ => unreachable!(),
    }
}

fn make_edge_parameters(
    edge_definition: &FieldDefinition,
    specified_arguments: &BTreeMap<Arc<str>, FieldValue>,
) -> Result<EdgeParameters, Vec<FrontendError>> {
    let mut errors: Vec<FrontendError> = vec![];

    let mut edge_arguments: BTreeMap<Arc<str>, FieldValue> = BTreeMap::new();
    for arg in &edge_definition.arguments {
        let arg_name = arg.node.name.node.as_ref();
        let specified_value = match specified_arguments.get(arg_name) {
            None => {
                // Argument value was not specified.
                // If there's an explicit default defined in the schema, use it.
                // Otherwise, if the parameter is nullable, use an implicit "null" default.
                // All other cases are an error.
                arg.node
                    .default_value
                    .as_ref()
                    .map(|v| {
                        let value = FieldValue::try_from(v.node.clone()).unwrap();

                        // The default value must be a valid type for the parameter,
                        // otherwise the schema itself is invalid.
                        assert!(Type::from_type(&arg.node.ty.node).is_valid_value(&value));

                        value
                    })
                    .or({
                        if arg.node.ty.node.nullable {
                            Some(FieldValue::Null)
                        } else {
                            None
                        }
                    })
            }
            Some(value) => {
                // Type-check the supplied value against the schema.
                if !Type::from_type(&arg.node.ty.node).is_valid_value(value) {
                    errors.push(FrontendError::InvalidEdgeParameterType(
                        arg_name.to_string(),
                        edge_definition.name.node.to_string(),
                        arg.node.ty.to_string(),
                        value.clone(),
                    ));
                }
                Some(value.clone())
            }
        };

        match specified_value {
            None => {
                errors.push(FrontendError::MissingRequiredEdgeParameter(
                    arg_name.to_string(),
                    edge_definition.name.node.to_string(),
                ));
            }
            Some(value) => {
                edge_arguments.insert_or_error(arg_name.to_owned().into(), value).unwrap();
                // Duplicates should have been caught at parse time.
            }
        }
    }

    // Check whether any of the supplied parameters aren't expected by the schema.
    for specified_argument_name in specified_arguments.keys() {
        if !edge_arguments.contains_key(specified_argument_name) {
            // This edge parameter isn't defined expected in the schema,
            // and it's an error to supply it.
            errors.push(FrontendError::UnexpectedEdgeParameter(
                specified_argument_name.to_string(),
                edge_definition.name.node.to_string(),
            ))
        }
    }

    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(EdgeParameters::new(Arc::new(edge_arguments)))
    }
}

#[allow(clippy::too_many_arguments)]
fn make_local_field_filter_expr(
    schema: &Schema,
    component_path: &ComponentPath,
    tags: &mut TagHandler<'_>,
    current_vertex_vid: Vid,
    property_name: &Arc<str>,
    property_type: &Type,
    filter_directive: &FilterDirective,
) -> Result<Operation<OperationSubject, Argument>, Vec<FrontendError>> {
    let left = LocalField { field_name: property_name.clone(), field_type: property_type.clone() };

    filters::make_filter_expr(
        schema,
        component_path,
        tags,
        current_vertex_vid,
        OperationSubject::LocalField(left),
        filter_directive,
    )
}

pub fn make_ir_for_query(schema: &Schema, query: &Query) -> Result<IRQuery, FrontendError> {
    validate_query_against_schema(schema, query)?;

    let mut vid_maker = successors(Some(Vid::new(NonZeroUsize::new(1).unwrap())), |x| {
        let inner_number = x.0.get();
        Some(Vid::new(NonZeroUsize::new(inner_number.checked_add(1).unwrap()).unwrap()))
    });
    let mut eid_maker = successors(Some(Eid::new(NonZeroUsize::new(1).unwrap())), |x| {
        let inner_number = x.0.get();
        Some(Eid::new(NonZeroUsize::new(inner_number.checked_add(1).unwrap()).unwrap()))
    });

    let mut errors: Vec<FrontendError> = vec![];

    let (root_field_name, root_field_pre_coercion_type, root_field_post_coercion_type, _) =
        get_field_name_and_type_from_schema(&schema.query_type.fields, &query.root_field);
    let starting_vid = vid_maker.next().unwrap();

    let root_parameters = make_edge_parameters(
        get_edge_definition_from_schema(schema, schema.query_type_name(), root_field_name),
        &query.root_connection.arguments,
    );

    let mut component_path = ComponentPath::new(starting_vid);
    let mut tags = Default::default();
    let mut output_handler = OutputHandler::new(starting_vid, None);
    let mut root_component = make_query_component(
        schema,
        query,
        &mut vid_maker,
        &mut eid_maker,
        &mut component_path,
        &mut output_handler,
        &mut tags,
        None,
        starting_vid,
        root_field_pre_coercion_type,
        root_field_post_coercion_type,
        &query.root_field,
    );

    if let Err(e) = &root_parameters {
        errors.extend(e.iter().cloned());
    }

    let root_component = match root_component {
        Ok(r) => r,
        Err(e) => {
            errors.extend(e);
            return Err(errors.into());
        }
    };
    let mut variables: BTreeMap<Arc<str>, Type> = Default::default();
    if let Err(v) = fill_in_query_variables(&mut variables, &root_component) {
        errors.extend(v.into_iter().map(|x| x.into()));
    }

    if let Err(e) = tags.finish() {
        errors.push(FrontendError::UnusedTags(e.into_iter().map(String::from).collect()));
    }

    let all_outputs = output_handler.finish();
    if let Err(duplicates) = check_for_duplicate_output_names(all_outputs) {
        let (all_vertices, all_folds) = collect_ir_vertices_and_folds(&root_component);
        let errs = make_duplicated_output_names_error(&all_vertices, &all_folds, duplicates);
        errors.extend(errs);
    }

    if errors.is_empty() {
        Ok(IRQuery {
            root_name: root_field_name.into(),
            root_parameters: root_parameters.unwrap(),
            root_component: root_component.into(),
            variables,
        })
    } else {
        Err(errors.into())
    }
}

fn collect_ir_vertices_and_folds(
    root_component: &IRQueryComponent,
) -> (BTreeMap<Vid, IRVertex>, BTreeMap<Eid, Arc<IRFold>>) {
    let mut vertices = Default::default();
    let mut folds = Default::default();
    collect_ir_vertices_and_folds_recursive_step(&mut vertices, &mut folds, root_component);
    (vertices, folds)
}

fn collect_ir_vertices_and_folds_recursive_step(
    vertices: &mut BTreeMap<Vid, IRVertex>,
    folds: &mut BTreeMap<Eid, Arc<IRFold>>,
    component: &IRQueryComponent,
) {
    vertices.extend(component.vertices.iter().map(|(k, v)| (*k, v.clone())));

    component.folds.iter().for_each(move |(eid, fold)| {
        folds.insert(*eid, Arc::clone(fold));

        collect_ir_vertices_and_folds_recursive_step(vertices, folds, &fold.component);
    })
}

fn fill_in_query_variables(
    variables: &mut BTreeMap<Arc<str>, Type>,
    component: &IRQueryComponent,
) -> Result<(), Vec<FilterTypeError>> {
    let mut errors: Vec<FilterTypeError> = vec![];

    let all_variable_uses = component
        .vertices
        .values()
        .flat_map(|vertex| &vertex.filters)
        .map(|filter| filter.right())
        .chain(
            component
                .folds
                .values()
                .flat_map(|fold| &fold.post_filters)
                .map(|filter| filter.right()),
        )
        .filter_map(|rhs| match rhs {
            Some(Argument::Variable(vref)) => Some(vref),
            _ => None,
        });
    for vref in all_variable_uses {
        let existing_type = variables
            .entry(vref.variable_name.clone())
            .or_insert_with(|| vref.variable_type.clone());

        match existing_type.intersect(&vref.variable_type) {
            Some(intersection) => {
                *existing_type = intersection;
            }
            None => {
                errors.push(FilterTypeError::IncompatibleVariableTypeRequirements(
                    vref.variable_name.to_string(),
                    existing_type.to_string(),
                    vref.variable_type.to_string(),
                ));
            }
        }
    }

    for fold in component.folds.values() {
        if let Err(e) = fill_in_query_variables(variables, fold.component.as_ref()) {
            errors.extend(e);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn make_duplicated_output_names_error(
    ir_vertices: &BTreeMap<Vid, IRVertex>,
    folds: &BTreeMap<Eid, Arc<IRFold>>,
    duplicates: BTreeMap<Arc<str>, Vec<FieldRef>>,
) -> Vec<FrontendError> {
    let conflict_info = DuplicatedNamesConflict {
        duplicates: duplicates
            .iter()
            .map(|(k, fields)| {
                let duplicate_values = fields
                    .iter()
                    .map(|field| {
                        let field_name = describe_field_ref(field);

                        let type_name = match field {
                            FieldRef::ContextField(field) => {
                                let vid = field.vertex_id;
                                ir_vertices[&vid].type_name.to_string()
                            }
                            FieldRef::FoldSpecificField(field) => match field.kind {
                                FoldSpecificFieldKind::Count => {
                                    folds[&field.fold_eid].edge_name.to_string()
                                }
                            },
                            FieldRef::TransformedField(transformed) => {
                                match &transformed.value.base {
                                    TransformBase::ContextField(field) => {
                                        let vid = field.vertex_id;
                                        ir_vertices[&vid].type_name.to_string()
                                    }
                                    TransformBase::FoldSpecificField(field) => {
                                        let vid = field.fold_root_vid;
                                        match field.kind {
                                            FoldSpecificFieldKind::Count => {
                                                folds[&field.fold_eid].edge_name.to_string()
                                            }
                                        }
                                    }
                                }
                            }
                        };

                        (type_name, field_name)
                    })
                    .collect();
                (k.to_string(), duplicate_values)
            })
            .collect(),
    };
    vec![FrontendError::MultipleOutputsWithSameName(conflict_info)]
}

#[allow(clippy::type_complexity)]
fn check_for_duplicate_output_names(
    maybe_duplicated_outputs: BTreeMap<Arc<str>, Vec<FieldRef>>,
) -> Result<BTreeMap<Arc<str>, FieldRef>, BTreeMap<Arc<str>, Vec<FieldRef>>> {
    maybe_duplicated_outputs
        .into_iter()
        .flat_map(|(name, outputs)| outputs.into_iter().map(move |o| (name.clone(), o)))
        .try_collect_unique()
}

#[allow(clippy::too_many_arguments)]
fn make_query_component<'schema, 'query, V, E>(
    schema: &'schema Schema,
    query: &'query Query,
    vid_maker: &mut V,
    eid_maker: &mut E,
    component_path: &mut ComponentPath,
    output_handler: &mut OutputHandler<'query>,
    tags: &mut TagHandler<'query>,
    parent_vid: Option<Vid>,
    starting_vid: Vid,
    pre_coercion_type: Arc<str>,
    post_coercion_type: Arc<str>,
    starting_field: &'query FieldNode,
) -> Result<IRQueryComponent, Vec<FrontendError>>
where
    'schema: 'query,
    V: Iterator<Item = Vid>,
    E: Iterator<Item = Eid>,
{
    let mut errors: Vec<FrontendError> = vec![];

    // Vid -> (vertex type, node that represents it)
    let mut vertices: BTreeMap<Vid, (Arc<str>, &'query FieldNode)> = Default::default();

    // Eid -> (from vid, to vid, connection that represents it)
    let mut edges: BTreeMap<Eid, (Vid, Vid, &'query FieldConnection)> = Default::default();

    // Vid -> vec of property names at that vertex used in the query
    let mut property_names_by_vertex: BTreeMap<Vid, Vec<Arc<str>>> = Default::default();

    // (Vid, property name) -> (property name, property type, nodes that represent the property)
    #[allow(clippy::type_complexity)]
    let mut properties: BTreeMap<
        (Vid, Arc<str>),
        (Arc<str>, Type, SmallVec<[&'query FieldNode; 1]>),
    > = Default::default();

    output_handler.begin_subcomponent();

    let mut folds: BTreeMap<Eid, Arc<IRFold>> = Default::default();
    if let Err(e) = fill_in_vertex_data(
        schema,
        query,
        vid_maker,
        eid_maker,
        &mut vertices,
        &mut edges,
        &mut folds,
        &mut property_names_by_vertex,
        &mut properties,
        component_path,
        output_handler,
        tags,
        None,
        starting_vid,
        pre_coercion_type,
        post_coercion_type,
        starting_field,
    ) {
        errors.extend(e);
    }

    let vertex_results = vertices.iter().map(|(vid, (uncoerced_type_name, field_node))| {
        make_vertex(
            schema,
            &property_names_by_vertex,
            &properties,
            tags,
            component_path,
            *vid,
            uncoerced_type_name,
            field_node,
        )
    });

    let ir_vertices: BTreeMap<Vid, IRVertex> = vertex_results
        .filter_map(|res| match res {
            Ok(v) => Some((v.vid, v)),
            Err(e) => {
                errors.extend(e);
                None
            }
        })
        .try_collect_unique()
        .unwrap();
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut ir_edges: BTreeMap<Eid, Arc<IREdge>> = BTreeMap::new();
    for (eid, (from_vid, to_vid, field_connection)) in edges.iter() {
        let from_vertex_type = &ir_vertices[from_vid].type_name;
        let edge_definition = get_edge_definition_from_schema(
            schema,
            from_vertex_type.as_ref(),
            field_connection.name.as_ref(),
        );
        let edge_name = edge_definition.name.node.as_ref().to_owned().into();

        let parameters_result = make_edge_parameters(edge_definition, &field_connection.arguments);

        let optional = field_connection.optional.is_some();
        let recursive = match field_connection.recurse.as_ref() {
            None => None,
            Some(d) => {
                match get_recurse_implicit_coercion(
                    schema,
                    &ir_vertices[from_vid],
                    edge_definition,
                    d,
                ) {
                    Ok(coerce_to) => Some(Recursive::new(d.depth, coerce_to)),
                    Err(e) => {
                        errors.push(e);
                        None
                    }
                }
            }
        };

        match parameters_result {
            Ok(parameters) => {
                ir_edges.insert(
                    *eid,
                    IREdge {
                        eid: *eid,
                        from_vid: *from_vid,
                        to_vid: *to_vid,
                        edge_name,
                        parameters,
                        optional,
                        recursive,
                    }
                    .into(),
                );
            }
            Err(e) => {
                errors.extend(e);
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let maybe_duplicated_outputs = output_handler.end_subcomponent();

    let component_outputs = match check_for_duplicate_output_names(maybe_duplicated_outputs) {
        Ok(outputs) => outputs,
        Err(duplicates) => {
            return Err(make_duplicated_output_names_error(&ir_vertices, &folds, duplicates))
        }
    };

    // TODO: fixme, temporary hack to avoid changing the IRQueryComponent struct
    let hacked_outputs = component_outputs
        .into_iter()
        .filter(|(_, v)| match &v {
            FieldRef::ContextField(..) => true,
            FieldRef::FoldSpecificField(..) => false,
            FieldRef::TransformedField(inner) => {
                !matches!(inner.value.base, TransformBase::FoldSpecificField(..))
            }
        })
        .collect();

    Ok(IRQueryComponent {
        root: starting_vid,
        vertices: ir_vertices,
        edges: ir_edges,
        folds,
        outputs: hacked_outputs,
    })
}

/// Four possible cases exist for the relationship between the `from_vid` vertex type
/// and the destination type of the edge as defined on the field representing it.
/// Let's the `from_vid` vertex type be S for "source,"
/// let the recursed edge's field name be `e` for "edge,"
/// and let the vertex type within the S.e field be called D for "destination.""
/// 1. The two types S and D are completely unrelated.
/// 2. S is a strict supertype of D.
/// 3. S is equal to D.
/// 4. S is a strict subtype of D.
///
/// Cases 1. and 2. return Err: recursion starts at depth = 0, so the `from_vid` vertex
/// must be assigned to the D-typed scope within the S.e field, which is a type error
/// due to the incompatible types of S and D.
///
/// Case 3 is Ok and is straightforward.
///
/// Case 4 may be Ok and may be Err, and if Ok it *may* require an implicit coercion.
/// If D has a D.e field, two sub-cases are possible:
/// 4a. D has a D.e field pointing to a vertex type of D. (Due to schema validity,
///     D.e cannot point to a subtype of D since S.e must be an equal or narrower type
///     than D.e.)
///     This case is Ok and does not require an implicit coercion: the desired edge
///     exists at all types encountered in the recursion.
/// 4b. D has a D.e field, but it points to a vertex type that is a supertype of D.
///     This would require another implicit coercion after expanding D.e
///     (i.e. when recursing from depth = 2+) and may require more coercions
///     deeper still since the depth = 1 point of view is analogous to case 4 as a whole.
///     This case is currently not supported and will produce Err, but may become
///     supported in the future.
///     (Note that D.e cannot point to a completely unrelated type, since S is a subtype
///      of D and therefore S.e must be an equal or narrower type than D.e or else
///      the schema is not valid.)
///
/// If D does not have a D.e field, two more sub-cases are possible:
/// 4c. D does not have a D.e field, but the S.e field has an unambiguous origin type:
///     there's exactly one type X, subtype of D and either supertype of or equal to S,
///     which defines X.e and from which S.e is derived. Again, due to schema validity,
///     S.e must be an equal or narrower type than X.e, so the vertex type within X.e
///     must be either equal to or a supertype of D, the vertex type of S.e.
///     - If a supertype of D, this currently returns Err and is not supported because
///       of the same reason as case 4b. It may be supported in the future.
///     - If X.e has a vertex type equal to D, this returns Ok and requires
///       an implicit coercion to X when recursing from depth = 1+.
/// 4d. D does not have a D.e field, and the S.e field has an ambiguous origin type:
///     there are at least two interfaces X and Y, where neither implements the other,
///     such that S implements both of them, and both the X.e and Y.e fields are defined.
///     In this case, it's not clear whether the implicit coercion should coerce
///     to X or to Y, so this is an Err.
fn get_recurse_implicit_coercion(
    schema: &Schema,
    from_vertex: &IRVertex,
    edge_definition: &FieldDefinition,
    d: &RecurseDirective,
) -> Result<Option<Arc<str>>, FrontendError> {
    let source_type = &from_vertex.type_name;
    let destination_type = get_underlying_named_type(&edge_definition.ty.node).as_ref();

    if !schema.is_named_type_subtype(destination_type, source_type) {
        // Case 1 or 2, Err() in both cases.
        // For the sake of a better error, we'll check which it is.
        if !schema.is_named_type_subtype(source_type, destination_type) {
            // Case 1, types are unrelated. Recursion on this edge is nonsensical.
            return Err(FrontendError::RecursingNonRecursableEdge(
                edge_definition.name.node.to_string(),
                source_type.to_string(),
                destination_type.to_string(),
            ));
        } else {
            // Case 2, the destination type is a subtype of the source type.
            // The vertex where the recursion starts might not "fit" in the depth = 0 recursion,
            // but the user could explicitly use a type coercion to coerce the starting vertex
            // into the destination type to make it work.
            return Err(FrontendError::RecursionToSubtype(
                edge_definition.name.node.to_string(),
                source_type.to_string(),
                destination_type.to_string(),
            ));
        }
    }

    if source_type.as_ref() == destination_type {
        // Case 3, Ok() and no coercion required.
        return Ok(None);
    }

    // Case 4, check whether the destination type also has an edge by that name.
    let edge_name: Arc<str> = Arc::from(edge_definition.name.node.as_ref());
    let destination_edge = schema.fields.get(&(Arc::from(destination_type), edge_name.clone()));
    match destination_edge {
        Some(destination_edge) => {
            // The destination type has that edge too.
            let edge_type = get_underlying_named_type(&destination_edge.ty.node).as_ref();
            if edge_type == destination_type {
                // Case 4a, Ok() and no coercion required.
                Ok(None)
            } else {
                // Case 4b, Err() because it's not supported yet.
                Err(FrontendError::EdgeRecursionNeedingMultipleCoercions(edge_name.to_string()))
            }
        }
        None => {
            // The destination type doesn't have that edge. Try to find a unique implicit coercion
            // to a type that does have that edge so we can make the recursion work.
            let edge_origin = &schema.field_origins[&(source_type.clone(), edge_name.clone())];
            match edge_origin {
                FieldOrigin::SingleAncestor(ancestor) => {
                    // Case 4c, check the ancestor type's edge field type for two more sub-cases.
                    let ancestor_edge = &schema.fields[&(ancestor.clone(), edge_name.clone())];
                    let edge_type = get_underlying_named_type(&ancestor_edge.ty.node).as_ref();
                    if edge_type == destination_type {
                        // A single implicit coercion to the ancestor type will work here.
                        Ok(Some(ancestor.clone()))
                    } else {
                        Err(FrontendError::EdgeRecursionNeedingMultipleCoercions(
                            edge_name.to_string(),
                        ))
                    }
                }
                FieldOrigin::MultipleAncestors(multiple) => {
                    // Case 4d, Err() because we can't figure out which implicit coercion to use.
                    Err(FrontendError::AmbiguousOriginEdgeRecursion(edge_name.to_string()))
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn make_vertex<'query>(
    schema: &Schema,
    property_names_by_vertex: &BTreeMap<Vid, Vec<Arc<str>>>,
    properties: &BTreeMap<(Vid, Arc<str>), (Arc<str>, Type, SmallVec<[&'query FieldNode; 1]>)>,
    tags: &mut TagHandler<'_>,
    component_path: &ComponentPath,
    vid: Vid,
    uncoerced_type_name: &Arc<str>,
    field_node: &'query FieldNode,
) -> Result<IRVertex, Vec<FrontendError>> {
    let mut errors: Vec<FrontendError> = vec![];

    // If the current vertex is the root of a `@fold`, then sometimes outputs are allowed.
    // This will be handled and checked in the fold creation function, so ignore it here.
    //
    // If the current vertex is not the root of a fold, then outputs are not allowed
    // and we should report an error.
    let is_fold_root = component_path.is_component_root(vid);
    if !is_fold_root && !field_node.output.is_empty() {
        errors.push(FrontendError::UnsupportedEdgeOutput(field_node.name.as_ref().to_owned()));
    }

    if let Some(first_filter) = field_node.filter.first() {
        // TODO: If @filter on edges is allowed, tweak this.
        errors.push(FrontendError::UnsupportedEdgeFilter(field_node.name.as_ref().to_owned()));
    }

    if let Some(first_tag) = field_node.tag.first() {
        // TODO: If @tag on edges is allowed, tweak this.
        errors.push(FrontendError::UnsupportedEdgeTag(field_node.name.as_ref().to_owned()));
    }

    let default_func = || {
        Result::<(Arc<str>, Option<Arc<str>>), FrontendError>::Ok((
            uncoerced_type_name.clone(),
            None,
        ))
    };
    let mapper_func = |coerced_to_type: Arc<str>| {
        let coerced_type =
            get_vertex_type_definition_from_schema(schema, coerced_to_type.as_ref())?;
        Ok((coerced_type.name.node.as_ref().to_owned().into(), Some(uncoerced_type_name.clone())))
    };
    let (type_name, coerced_from_type) =
        match field_node.coerced_to.clone().map_or_else(default_func, mapper_func) {
            Ok(x) => x,
            Err(e) => {
                errors.push(e);
                return Err(errors);
            }
        };

    // Filters have to be processed here, and cannot be processed inside `fill_in_vertex_data()`.
    // This is because filters may reference data that won't be complete until all of
    // `fill_in_vertex_data()` has finished running: for example, if a filter on one property
    // in a given vertex references a tag produced from another property on the same vertex.
    //
    // For similar reasons, we have to process filters over transformed property values here too.
    let mut filters = vec![];
    for property_name in property_names_by_vertex.get(&vid).into_iter().flatten() {
        let (_, property_type, property_fields) =
            properties.get(&(vid, property_name.clone())).unwrap();

        for property_field in property_fields {
            for filter_directive in property_field.filter.iter() {
                match make_local_field_filter_expr(
                    schema,
                    component_path,
                    tags,
                    vid,
                    property_name,
                    property_type,
                    filter_directive,
                ) {
                    Ok(filter_operation) => {
                        filters.push(filter_operation);
                    }
                    Err(e) => {
                        errors.extend(e);
                    }
                }
            }

            let mut transforms: Vec<Transform> = vec![];
            let mut current_type = property_type.clone();
            let mut next_transform_group = property_field.transform_group.as_ref();
            while let Some(transform_group) = next_transform_group {
                let next_transform = match extract_property_like_transform_from_directive(
                    &transform_group.transform,
                    property_name,
                    &transforms,
                    &current_type,
                ) {
                    Ok(t) => t,
                    Err(e) => {
                        // This error should already have been reported while initially
                        // processing transforms for output and tag purposes
                        // in `fill_in_vertex_data()`.
                        // We have tests enforcing this.
                        //
                        // Ignore it here.
                        break;
                    }
                };
                current_type = match determine_transformed_field_type(current_type, &next_transform)
                {
                    Ok(t) => t,
                    Err(e) => {
                        // This error should already have been reported while initially
                        // processing transforms for output and tag purposes
                        // in `fill_in_vertex_data()`.
                        // We have tests enforcing this.
                        //
                        // Ignore it here.
                        break;
                    }
                };
                transforms.push(next_transform);

                for filter_directive in &transform_group.filter {
                    let base = ContextField {
                        field_name: property_name.clone(),
                        field_type: property_type.clone(),
                        vertex_id: vid,
                    };
                    let transformed_field = TransformedField {
                        value: Arc::new(TransformedValue {
                            base: TransformBase::ContextField(base),
                            transforms: transforms.clone(),
                        }),
                        tid: transform_group.tid,
                        field_type: current_type.clone(),
                    };

                    match filters::make_filter_expr(
                        schema,
                        component_path,
                        tags,
                        vid,
                        OperationSubject::TransformedField(transformed_field),
                        filter_directive,
                    ) {
                        Ok(filter_operation) => {
                            filters.push(filter_operation);
                        }
                        Err(e) => {
                            errors.extend(e);
                        }
                    }
                }

                next_transform_group = transform_group.retransform.as_deref();
            }
        }
    }

    if errors.is_empty() {
        Ok(IRVertex { vid, type_name, coerced_from_type, filters })
    } else {
        Err(errors)
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn fill_in_vertex_data<'schema, 'query, V, E>(
    schema: &'schema Schema,
    query: &'query Query,
    vid_maker: &mut V,
    eid_maker: &mut E,
    vertices: &mut BTreeMap<Vid, (Arc<str>, &'query FieldNode)>,
    edges: &mut BTreeMap<Eid, (Vid, Vid, &'query FieldConnection)>,
    folds: &mut BTreeMap<Eid, Arc<IRFold>>,
    property_names_by_vertex: &mut BTreeMap<Vid, Vec<Arc<str>>>,
    properties: &mut BTreeMap<(Vid, Arc<str>), (Arc<str>, Type, SmallVec<[&'query FieldNode; 1]>)>,
    component_path: &mut ComponentPath,
    output_handler: &mut OutputHandler<'query>,
    tags: &mut TagHandler<'query>,
    parent_vid: Option<Vid>,
    current_vid: Vid,
    pre_coercion_type: Arc<str>,
    post_coercion_type: Arc<str>,
    current_field: &'query FieldNode,
) -> Result<(), Vec<FrontendError>>
where
    'schema: 'query,
    V: Iterator<Item = Vid>,
    E: Iterator<Item = Eid>,
{
    let mut errors: Vec<FrontendError> = vec![];

    vertices.insert_or_error(current_vid, (pre_coercion_type, current_field)).unwrap();

    let defined_fields = get_vertex_field_definitions(schema, post_coercion_type.as_ref());

    for (connection, subfield) in &current_field.connections {
        let (
            subfield_name,
            subfield_pre_coercion_type,
            subfield_post_coercion_type,
            subfield_raw_type,
        ) = get_field_name_and_type_from_schema(defined_fields, subfield);
        if schema.vertex_types.contains_key(subfield_post_coercion_type.as_ref()) {
            // Processing an edge.

            let next_vid = vid_maker.next().unwrap();
            let next_eid = eid_maker.next().unwrap();
            output_handler
                .begin_nested_scope(next_vid, subfield.alias.as_ref().map(|x| x.as_ref()));

            // Ensure we don't have `@transform` applied to the edge directly,
            // either completely without `@fold` or placed before it.
            // Both cases are an error, though a different error for each for better UX.
            if let Some(transform_group) = &subfield.transform_group {
                if connection.fold.is_none() {
                    // `@transform` used without `@fold` on the edge at all.
                    TransformTypeError::add_errors_for_transform_used_on_unfolded_edge(
                        subfield_name,
                        &transform_group.transform,
                        &mut errors,
                    );
                } else {
                    // `@transform` placed before `@fold` on the edge.
                    unreachable!("@transform placed before @fold, but this error wasn't caught in the parser")
                }
            }

            if let Some(fold_group) = &connection.fold {
                if connection.optional.is_some() {
                    errors.push(FrontendError::UnsupportedDirectiveOnFoldedEdge(
                        subfield.name.to_string(),
                        "@optional".to_owned(),
                    ));
                }
                if connection.recurse.is_some() {
                    errors.push(FrontendError::UnsupportedDirectiveOnFoldedEdge(
                        subfield.name.to_string(),
                        "@recurse".to_owned(),
                    ));
                }

                let edge_definition = get_edge_definition_from_schema(
                    schema,
                    post_coercion_type.as_ref(),
                    connection.name.as_ref(),
                );
                match make_edge_parameters(edge_definition, &connection.arguments) {
                    Ok(edge_parameters) => {
                        match make_fold(
                            schema,
                            query,
                            vid_maker,
                            eid_maker,
                            component_path,
                            output_handler,
                            tags,
                            fold_group,
                            next_eid,
                            edge_definition.name.node.as_str().to_owned().into(),
                            edge_parameters,
                            current_vid,
                            next_vid,
                            subfield_pre_coercion_type,
                            subfield_post_coercion_type,
                            subfield,
                        ) {
                            Ok(fold) => {
                                folds.insert(next_eid, fold.into());
                            }
                            Err(e) => {
                                errors.extend(e);
                            }
                        }
                    }
                    Err(e) => {
                        errors.extend(e);
                    }
                }
            } else {
                edges
                    .insert_or_error(next_eid, (current_vid, next_vid, connection))
                    .expect("Unexpectedly encountered duplicate eid");

                if let Err(e) = fill_in_vertex_data(
                    schema,
                    query,
                    vid_maker,
                    eid_maker,
                    vertices,
                    edges,
                    folds,
                    property_names_by_vertex,
                    properties,
                    component_path,
                    output_handler,
                    tags,
                    Some(current_vid),
                    next_vid,
                    subfield_pre_coercion_type.clone(),
                    subfield_post_coercion_type.clone(),
                    subfield,
                ) {
                    errors.extend(e);
                }
            }

            output_handler.end_nested_scope(next_vid);
        } else if get_builtin_scalars().contains(subfield_post_coercion_type.as_ref())
            || schema.scalars.contains_key(subfield_post_coercion_type.as_ref())
            || subfield_name == TYPENAME_META_FIELD
        {
            // Processing a property.
            fill_in_property_data(
                property_names_by_vertex,
                properties,
                component_path,
                output_handler,
                tags,
                current_vid,
                subfield,
                connection,
                subfield_name,
                subfield_raw_type,
                &mut errors,
            );
        } else {
            unreachable!("field name: {}", subfield_name);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
fn fill_in_property_data<'query>(
    property_names_by_vertex: &mut BTreeMap<Vid, Vec<Arc<str>>>,
    properties: &mut BTreeMap<(Vid, Arc<str>), (Arc<str>, Type, SmallVec<[&'query FieldNode; 1]>)>,
    component_path: &mut ComponentPath,
    output_handler: &mut OutputHandler<'query>,
    tags: &mut TagHandler<'query>,
    current_vid: Vid,
    property_node: &'query FieldNode,
    connection: &'query FieldConnection,
    property_name: &str,
    property_type: Type,
    errors: &mut Vec<FrontendError>,
) {
    // @fold is not allowed on a property
    if connection.fold.is_some() {
        errors.push(FrontendError::UnsupportedDirectiveOnProperty(
            "@fold".into(),
            property_node.name.to_string(),
        ));
    }

    // @optional is not allowed on a property
    if connection.optional.is_some() {
        errors.push(FrontendError::UnsupportedDirectiveOnProperty(
            "@optional".into(),
            property_node.name.to_string(),
        ));
    }

    // @recurse is not allowed on a property
    if connection.recurse.is_some() {
        errors.push(FrontendError::UnsupportedDirectiveOnProperty(
            "@recurse".into(),
            property_node.name.to_string(),
        ));
    }

    let property_name: Arc<str> = property_name.into();
    let key = (current_vid, property_name.clone());
    properties
        .entry(key)
        .and_modify(|(prior_name, prior_type, subfields)| {
            assert_eq!(property_name.as_ref(), prior_name.as_ref());
            assert_eq!(&property_type, prior_type);
            subfields.push(property_node);
        })
        .or_insert_with(|| {
            property_names_by_vertex.entry(current_vid).or_default().push(property_name.clone());

            (property_name, property_type.clone(), SmallVec::from([property_node]))
        });

    let mut transforms: Vec<Transform> = vec![];
    let mut current_tid: Option<Tid> = None;
    let mut current_type = property_type.clone();
    let mut output_directives = property_node.output.as_slice();
    let mut tag_directives = property_node.tag.as_slice();
    let mut next_transform_group = property_node.transform_group.as_ref();

    loop {
        for output_directive in output_directives {
            let field_ref = make_field_ref_for_possibly_transformed_property(
                current_vid,
                &property_node.name,
                &property_type,
                current_tid,
                &transforms,
                &current_type,
            );

            // The output's name can be either explicit or local (i.e. implicitly prefixed).
            // Explicit names are given explicitly in the directive:
            //     @output(name: "foo")
            // This would result in a "foo" output name, regardless of any prefixes.
            // Local names use the field's alias, if present, falling back to the field's name
            // otherwise. The local name is appended to any prefixes given as aliases
            // applied to the edges whose scopes enclose the output.
            if let Some(explicit_name) = output_directive.name.as_ref() {
                output_handler.register_explicitly_named_output(explicit_name.clone(), field_ref);
            } else {
                let local_name = property_node
                    .alias
                    .as_ref()
                    .map(|x| x.as_ref())
                    .unwrap_or_else(|| property_node.name.as_ref());
                output_handler.register_locally_named_output(
                    local_name,
                    Some(Box::new(transforms.iter().map(|t| t.operation_output_name()))),
                    field_ref,
                );
            }
        }

        for tag_directive in tag_directives {
            // The tag's name is the first of the following that is defined:
            // - the explicit "name" parameter in the @tag directive itself
            // - the alias of the field with the @tag directive
            // - the name of the field with the @tag directive
            let tag_name = tag_directive.name.as_ref().map(|x| x.as_ref()).unwrap_or_else(|| {
                property_node
                    .alias
                    .as_ref()
                    .map(|x| x.as_ref())
                    .unwrap_or_else(|| property_node.name.as_ref())
            });

            let tag_field = make_field_ref_for_possibly_transformed_property(
                current_vid,
                &property_node.name,
                &property_type,
                current_tid,
                &transforms,
                &current_type,
            );

            if let Err(e) = tags.register_tag(tag_name, tag_field, component_path) {
                errors.push(FrontendError::MultipleTagsWithSameName(tag_name.to_string()));
            }
        }

        if let Some(transform_group) = next_transform_group {
            let next_transform = match extract_property_like_transform_from_directive(
                &transform_group.transform,
                &property_node.name,
                &transforms,
                &current_type,
            ) {
                Ok(t) => t,
                Err(e) => {
                    errors.push(e.into());
                    break;
                }
            };
            current_tid = Some(transform_group.tid);
            output_directives = transform_group.output.as_slice();
            tag_directives = transform_group.tag.as_slice();
            next_transform_group = transform_group.retransform.as_deref();

            current_type = match determine_transformed_field_type(current_type, &next_transform) {
                Ok(t) => t,
                Err(e) => {
                    errors.push(e);
                    break;
                }
            };
            transforms.push(next_transform);
        } else {
            break;
        }
    }
}

fn make_field_ref_for_possibly_transformed_property(
    current_vid: Vid,
    property_name: &Arc<str>,
    property_type: &Type,
    current_tid: Option<Tid>,
    transforms: &[Transform],
    transformed_type: &Type,
) -> FieldRef {
    let context_field = ContextField {
        vertex_id: current_vid,
        field_name: Arc::clone(property_name),
        field_type: property_type.clone(),
    };
    if let Some(tid) = current_tid {
        FieldRef::TransformedField(TransformedField {
            value: Arc::new(TransformedValue {
                base: TransformBase::ContextField(context_field),
                // TODO: This `.to_owned()` is a full copy of the `Vec<Transform>`,
                //       so if we have a very deep chain of `@transform` directives where each
                //       intermediate result is used in a tag, filter, or output,
                //       that would result in O(n^2) copies.
                //
                //       In principle, we should be able to make a "smarter" data type here:
                //       we only ever append to the underlying `Vec<Transform>` as we process
                //       new `@transform` directives, so prior uses of it see an immutable
                //       prefix of the full final `Vec<Transform>`. This may lend itself to
                //       an `Arc`-ed shared-ownership prefix view over an underlying allocation.
                //       This is a potential future optimization opportunity!
                transforms: transforms.to_owned(),
            }),
            tid,
            field_type: transformed_type.clone(),
        })
    } else {
        FieldRef::ContextField(context_field)
    }
}

fn extract_property_like_transform_from_directive(
    transform_directive: &TransformDirective,
    property_name: &str,
    transforms_so_far: &[Transform],
    type_so_far: &Type,
) -> Result<Transform, TransformTypeError> {
    extract_transform_from_directive(transform_directive, type_so_far, || {
        TransformTypeError::fold_specific_transform_on_propertylike_value(
            transform_directive.kind.op_name(),
            property_name,
            transforms_so_far,
            type_so_far,
        )
    })
}

fn extract_transform_on_fold_count_from_directive(
    transform_directive: &TransformDirective,
    edge_name: &str,
    type_so_far: &Type,
) -> Result<Transform, TransformTypeError> {
    extract_transform_from_directive(transform_directive, type_so_far, || {
        // A fold-specific filter is used *after* a fold-count value has already been created.
        // For example: `some_edge @fold @transform(op: "count") @transform(op: "count")`
        //                                                       ^^^^^^^^^^^^^^^^^^^^^^^
        //                                                       we are here, this is the error
        TransformTypeError::duplicated_count_transform_on_folded_edge(edge_name)
    })
}

fn extract_transform_from_directive(
    transform_directive: &TransformDirective,
    type_so_far: &Type,
    err_func: impl FnOnce() -> TransformTypeError,
) -> Result<Transform, TransformTypeError> {
    match &transform_directive.kind {
        TransformOp::Len => Ok(Transform::Len),
        TransformOp::Abs => Ok(Transform::Abs),
        TransformOp::Add(arg) => Ok(Transform::Add(resolve_transform_argument(arg)?)),
        TransformOp::AddF(arg) => Ok(Transform::AddF(resolve_transform_argument(arg)?)),
        TransformOp::Count => Err(err_func()),
    }
}

fn resolve_transform_argument(arg: &OperatorArgument) -> Result<Argument, TransformTypeError> {
    todo!()
}

fn determine_transformed_field_type(
    initial_type: Type,
    next_transform: &Transform,
) -> Result<Type, FrontendError> {
    // TODO: refactor type-checking errors into helpers + three categories:
    // - inappropriate left-hand type for transform
    // - inappropriate tag type for transform
    // - inappropriate (conflicting) variable type for transform;
    //   for this last one, ensure inferring `Int` vs `Int!` narrows to `Int!` correctly
    //   but other inference mismatches get reported as errors just like from filters
    match next_transform {
        Transform::Len => {
            if initial_type.is_list() {
                Ok(Type::new_named_type("Int", initial_type.nullable()))
            } else {
                Err(todo!())
            }
        }
        Transform::Abs => {
            let base_type = initial_type.base_type();
            if base_type == "Int" || base_type == "Float" {
                Ok(initial_type)
            } else {
                Err(todo!())
            }
        }
        Transform::Add(op) => {
            match op {
                Argument::Tag(tag) => {
                    let op_type = tag.field_type();
                    if op_type.base_type() != "Int" {
                        return Err(todo!());
                    }
                }
                Argument::Variable(var) => {
                    let op_type = &var.variable_type;
                    if op_type.base_type() != "Int" {
                        return Err(todo!());
                    }
                }
            };
            Ok(Type::new_named_type("Int", initial_type.nullable()))
        }
        Transform::AddF(op) => {
            match op {
                Argument::Tag(tag) => {
                    let op_type = tag.field_type();
                    if op_type.base_type() != "Float" {
                        return Err(todo!());
                    }
                }
                Argument::Variable(var) => {
                    let op_type = &var.variable_type;
                    if op_type.base_type() != "Float" {
                        return Err(todo!());
                    }
                }
            };
            Ok(Type::new_named_type("Float", initial_type.nullable()))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn make_fold<'schema, 'query, V, E>(
    schema: &'schema Schema,
    query: &'query Query,
    vid_maker: &mut V,
    eid_maker: &mut E,
    component_path: &mut ComponentPath,
    output_handler: &mut OutputHandler<'query>,
    tags: &mut TagHandler<'query>,
    fold_group: &'query FoldGroup,
    fold_eid: Eid,
    edge_name: Arc<str>,
    edge_parameters: EdgeParameters,
    parent_vid: Vid,
    starting_vid: Vid,
    starting_pre_coercion_type: Arc<str>,
    starting_post_coercion_type: Arc<str>,
    starting_field: &'query FieldNode,
) -> Result<IRFold, Vec<FrontendError>>
where
    'schema: 'query,
    V: Iterator<Item = Vid>,
    E: Iterator<Item = Eid>,
{
    component_path.push(starting_vid);
    tags.begin_subcomponent(starting_vid);

    let mut errors = vec![];
    let component = make_query_component(
        schema,
        query,
        vid_maker,
        eid_maker,
        component_path,
        output_handler,
        tags,
        Some(parent_vid),
        starting_vid,
        starting_pre_coercion_type,
        starting_post_coercion_type,
        starting_field,
    )?;
    component_path.pop(starting_vid);
    let imported_tags = tags.end_subcomponent(starting_vid);

    if !starting_field.output.is_empty() {
        // The edge has @fold @output but no @transform.
        // If it had a @transform then the output would have been in the field's transform group.
        errors.push(FrontendError::UnsupportedEdgeOutput(starting_field.name.as_ref().to_owned()));
    }

    let mut post_filters = vec![];
    let mut fold_specific_outputs = BTreeMap::new();
    let mut maybe_base_field: Option<FoldSpecificField> = None;
    let mut subsequent_transforms = vec![];
    let mut current_type = Type::new_named_type("Int", false);
    let mut next_transform_group = fold_group.transform.as_ref();

    while let Some(transform_group) = next_transform_group {
        next_transform_group = transform_group.retransform.as_deref();

        let (field_ref, subject) = if let Some(base_field) = maybe_base_field.as_ref() {
            let next_transform = match extract_transform_on_fold_count_from_directive(
                &transform_group.transform,
                edge_name.as_ref(),
                &current_type,
            ) {
                Ok(t) => t,
                Err(e) => {
                    errors.push(e.into());
                    break;
                }
            };

            current_type = match determine_transformed_field_type(current_type, &next_transform) {
                Ok(t) => t,
                Err(e) => {
                    errors.push(e);
                    break;
                }
            };
            subsequent_transforms.push(next_transform);

            let transformed_field = TransformedField {
                value: Arc::new(TransformedValue {
                    base: TransformBase::FoldSpecificField(base_field.clone()),
                    // TODO: This `.clone()` is a full copy of the `Vec<Transform>`,
                    //       so if we have a very deep chain of `@transform` directives where each
                    //       intermediate result is used in a tag, filter, or output,
                    //       that would result in O(n^2) copies.
                    //
                    //       In principle, we should be able to make a "smarter" data type here:
                    //       we only ever append to the underlying `Vec<Transform>` as we process
                    //       new `@transform` directives, so prior uses of it see an immutable
                    //       prefix of the full final `Vec<Transform>`. This may lend itself to
                    //       an `Arc`-ed shared-ownership prefix view over an underlying allocation.
                    //       This is a potential future optimization opportunity!
                    transforms: subsequent_transforms.clone(),
                }),
                tid: transform_group.tid,
                field_type: current_type.clone(),
            };
            let field_ref = FieldRef::TransformedField(transformed_field.clone());
            let subject = OperationSubject::TransformedField(transformed_field);

            (field_ref, subject)
        } else {
            let fold_specific_field = match transform_group.transform.kind {
                TransformOp::Count => {
                    current_type = Type::new_named_type("Int", false);
                    FoldSpecificField {
                        fold_eid,
                        fold_root_vid: starting_vid,
                        kind: FoldSpecificFieldKind::Count,
                    }
                }
                _ => {
                    errors.push(
                        TransformTypeError::unsupported_transform_used_on_folded_edge(
                            &edge_name,
                            transform_group.transform.kind.op_name(),
                        )
                        .into(),
                    );
                    break;
                }
            };

            let field_ref = FieldRef::FoldSpecificField(fold_specific_field.clone());
            let subject = OperationSubject::FoldSpecificField(fold_specific_field.clone());
            maybe_base_field = Some(fold_specific_field);

            (field_ref, subject)
        };

        for filter_directive in &transform_group.filter {
            match make_filter_expr(
                schema,
                component_path,
                tags,
                starting_vid,
                subject.clone(),
                filter_directive,
            ) {
                Ok(filter) => post_filters.push(filter),
                Err(e) => errors.extend(e),
            }
        }
        for output in &transform_group.output {
            let final_output_name = match output.name.as_ref() {
                Some(explicit_name) => {
                    output_handler
                        .register_explicitly_named_output(explicit_name.clone(), field_ref.clone());
                    explicit_name.clone()
                }
                None => {
                    let local_name = if starting_field.alias.is_some() {
                        // The field has an alias already, so don't bother adding the edge name
                        // to the output name.
                        ""
                    } else {
                        // The field does not have an alias, so use the edge name as the base
                        // of the name.
                        starting_field.name.as_ref()
                    };
                    output_handler.register_locally_named_output(
                        local_name,
                        Some(Box::new([TransformOp::Count.op_name()].into_iter().chain(
                            subsequent_transforms.iter().map(|t| t.operation_output_name()),
                        ))),
                        field_ref.clone(),
                    )
                }
            };

            let prior_output_by_that_name =
                fold_specific_outputs.insert(final_output_name.clone(), field_ref.clone());
            if let Some(prior_output) = prior_output_by_that_name {
                let new_field_description =
                    describe_edge_with_fold_count_and_transforms(&subsequent_transforms);

                errors.push(FrontendError::MultipleOutputsWithSameName(DuplicatedNamesConflict {
                    duplicates: btreemap! {
                        final_output_name.to_string() => vec![
                            (edge_name.to_string(), describe_field_ref(&prior_output)),
                            (edge_name.to_string(), new_field_description),
                        ]
                    },
                }))
            }
        }
        for tag_directive in &transform_group.tag {
            let tag_name = tag_directive.name.as_ref().map(|x| x.as_ref());
            if let Some(tag_name) = tag_name {
                if let Err(e) = tags.register_tag(tag_name, field_ref.clone(), component_path) {
                    errors.push(FrontendError::MultipleTagsWithSameName(tag_name.to_string()));
                }
            } else {
                errors.push(FrontendError::explicit_tag_name_required(&subject))
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(IRFold {
        eid: fold_eid,
        from_vid: parent_vid,
        to_vid: starting_vid,
        edge_name,
        parameters: edge_parameters,
        component: component.into(),
        imported_tags,
        post_filters,
        fold_specific_outputs,
    })
}

fn describe_edge_with_fold_count_and_transforms(subsequent_transforms: &[Transform]) -> String {
    let mut buf = String::with_capacity(32);
    buf.write_str(FoldSpecificFieldKind::Count.field_name()).expect("write failed");
    for transform in subsequent_transforms {
        buf.write_char('.').expect("write failed");
        buf.write_str(transform.operation_output_name()).expect("write failed");
    }
    buf
}

fn describe_field_ref(field_ref: &FieldRef) -> String {
    match field_ref {
        FieldRef::ContextField(f) => f.field_name.to_string(),
        FieldRef::FoldSpecificField(f) => f.kind.field_name().to_string(),
        FieldRef::TransformedField(transformed) => {
            let mut buf = String::with_capacity(32);
            match &transformed.value.base {
                TransformBase::ContextField(f) => {
                    buf.write_str(&f.field_name).expect("write failed");
                }
                TransformBase::FoldSpecificField(f) => {
                    buf.write_str(f.kind.field_name()).expect("write failed");
                }
            }

            for transform in &transformed.value.transforms {
                buf.write_char('.').expect("write failed");
                buf.write_str(transform.operation_output_name()).expect("write failed");
            }
            buf
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::OnceLock,
    };

    use trustfall_filetests_macros::parameterize;

    use crate::{
        frontend::make_ir_for_query,
        schema::Schema,
        test_types::{TestIRQuery, TestIRQueryResult, TestParsedGraphQLQueryResult},
    };

    static FILESYSTEM_SCHEMA: OnceLock<Schema> = OnceLock::new();
    static NUMBERS_SCHEMA: OnceLock<Schema> = OnceLock::new();
    static NULLABLES_SCHEMA: OnceLock<Schema> = OnceLock::new();
    static RECURSES_SCHEMA: OnceLock<Schema> = OnceLock::new();

    fn get_filesystem_schema() -> &'static Schema {
        FILESYSTEM_SCHEMA.get_or_init(|| {
            Schema::parse(fs::read_to_string("test_data/schemas/filesystem.graphql").unwrap())
                .unwrap()
        })
    }

    fn get_numbers_schema() -> &'static Schema {
        NUMBERS_SCHEMA.get_or_init(|| {
            Schema::parse(fs::read_to_string("test_data/schemas/numbers.graphql").unwrap()).unwrap()
        })
    }

    fn get_nullables_schema() -> &'static Schema {
        NULLABLES_SCHEMA.get_or_init(|| {
            Schema::parse(fs::read_to_string("test_data/schemas/nullables.graphql").unwrap())
                .unwrap()
        })
    }

    fn get_recurses_schema() -> &'static Schema {
        RECURSES_SCHEMA.get_or_init(|| {
            Schema::parse(fs::read_to_string("test_data/schemas/recurses.graphql").unwrap())
                .unwrap()
        })
    }

    #[test]
    fn test_schemas_load_correctly() {
        // We want to merely touch the lazy-static variables so they get initialized.
        // If that succeeds, even very cursory checks will suffice.
        assert!(get_filesystem_schema().vertex_types.len() > 3);
        assert!(!get_numbers_schema().vertex_types.is_empty());
        assert!(!get_nullables_schema().vertex_types.is_empty());
        assert!(!get_recurses_schema().vertex_types.is_empty());
    }

    #[parameterize("trustfall_core/test_data/tests/frontend_errors")]
    fn frontend_errors(base: &Path, stem: &str) {
        parameterizable_tester(base, stem, ".frontend-error.ron")
    }

    #[parameterize("trustfall_core/test_data/tests/execution_errors")]
    fn execution_errors(base: &Path, stem: &str) {
        parameterizable_tester(base, stem, ".ir.ron")
    }

    #[parameterize("trustfall_core/test_data/tests/valid_queries")]
    fn valid_queries(base: &Path, stem: &str) {
        parameterizable_tester(base, stem, ".ir.ron")
    }

    fn parameterizable_tester(base: &Path, stem: &str, check_file_suffix: &str) {
        let mut input_path = PathBuf::from(base);
        input_path.push(format!("{stem}.graphql-parsed.ron"));

        let input_data = fs::read_to_string(input_path).unwrap();
        let test_query: TestParsedGraphQLQueryResult = ron::from_str(&input_data).unwrap();
        if test_query.is_err() {
            return;
        }
        let test_query = test_query.unwrap();

        let schema: &Schema = match test_query.schema_name.as_str() {
            "filesystem" => get_filesystem_schema(),
            "numbers" => get_numbers_schema(),
            "nullables" => get_nullables_schema(),
            "recurses" => get_recurses_schema(),
            _ => unimplemented!("unrecognized schema name: {:?}", test_query.schema_name),
        };

        let mut check_path = PathBuf::from(base);
        check_path.push(format!("{stem}{check_file_suffix}"));
        let check_data = fs::read_to_string(check_path).unwrap();

        let arguments = test_query.arguments;
        let constructed_test_item =
            make_ir_for_query(schema, &test_query.query).map(move |ir_query| TestIRQuery {
                schema_name: test_query.schema_name,
                ir_query,
                arguments,
            });

        let check_parsed: TestIRQueryResult = ron::from_str(&check_data).unwrap();

        assert_eq!(check_parsed, constructed_test_item);
    }
}
