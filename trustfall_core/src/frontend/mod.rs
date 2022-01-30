#![allow(dead_code, unused_variables, unused_mut)]
use std::{
    collections::BTreeMap,
    convert::TryFrom,
    iter::{self, successors},
    num::NonZeroUsize,
    sync::Arc,
};

use async_graphql_parser::{
    types::{BaseType, ExecutableDocument, FieldDefinition, Type, TypeDefinition, TypeKind},
    Positioned,
};
use async_graphql_value::Name;
use smallvec::SmallVec;

use crate::{
    graphql_query::{
        directives::{FilterDirective, FoldDirective, OperatorArgument, RecurseDirective},
        query::{parse_document, FieldConnection, FieldNode, Query},
    },
    ir::{
        indexed::IndexedQuery,
        types::{intersect_types, is_argument_type_valid},
        Argument, ContextField, EdgeParameters, Eid, FieldValue, IREdge, IRFold, IRQuery,
        IRQueryComponent, IRVertex, LocalField, Operation, Recursive, VariableRef, Vid,
    },
    schema::{FieldOrigin, Schema, BUILTIN_SCALARS},
    util::TryCollectUniqueKey,
};

use self::{
    error::{DuplicatedNamesConflict, FilterTypeError, FrontendError, ValidationError},
    util::get_underlying_named_type,
    validation::validate_query_against_schema,
};

pub mod error;
mod util;
mod validation;

pub fn parse(schema: &Schema, query: impl AsRef<str>) -> Result<Arc<IndexedQuery>, FrontendError> {
    let ir_query = parse_to_ir(schema, query)?;

    // .unwrap() must be safe here, since freshly-generated IRQuery objects must always
    // be safe to convert to IndexedQuery. This is a try_into() instead of into() because
    // IRQuery is Serialize/Deserialize and may therefore have been edited (e.g. by hand)
    // before being converted into IndexedQuery.
    let indexed_query: IndexedQuery = ir_query.try_into().unwrap();

    Ok(Arc::from(indexed_query))
}

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
) -> (&'a Name, Arc<str>, Arc<str>, &'a Type) {
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
                field_raw_type,
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
) -> Result<Option<Arc<EdgeParameters>>, Vec<FrontendError>> {
    let mut errors: Vec<FrontendError> = vec![];

    let mut edge_arguments: BTreeMap<Arc<str>, FieldValue> = BTreeMap::new();
    for arg in &edge_definition.arguments {
        let arg_name = arg.node.name.node.as_ref();
        let specified_value = match specified_arguments.get(arg_name) {
            None => {
                // Argument value was not specified, try to use a default if there is one.
                arg.node.default_value.as_ref().map(|v| {
                    let value = FieldValue::try_from(&v.node).unwrap();

                    // The default value must be a valid type for the parameter,
                    // otherwise the schema itself is invalid.
                    assert!(is_argument_type_valid(&arg.node.ty.node, &value));

                    value
                })
            }
            Some(value) => {
                // Type-check the supplied value against the schema.
                if !is_argument_type_valid(&arg.node.ty.node, value) {
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
                edge_arguments
                    .try_insert(arg_name.to_owned().into(), value)
                    .unwrap(); // Duplicates should have been caught at parse time.
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
    } else if edge_arguments.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Arc::new(EdgeParameters(edge_arguments))))
    }
}

fn infer_variable_type(
    property_name: &str,
    property_type: &Type,
    operation: &Operation<(), OperatorArgument>,
) -> Result<Type, FilterTypeError> {
    match operation {
        Operation::Equals(..) | Operation::NotEquals(..) => {
            // Direct equality comparison.
            // If the field is nullable, then the input should be nullable too
            // so that the null valued fields can be matched.
            Ok(property_type.clone())
        }
        Operation::LessThan(..)
        | Operation::LessThanOrEqual(..)
        | Operation::GreaterThan(..)
        | Operation::GreaterThanOrEqual(..) => {
            // The null value isn't orderable relative to non-null values of its type.
            // Use a type that is structurally the same but non-null at the top level.
            //
            // Why only the top level? Consider a comparison against type [[Int]].
            // Using a "null" valued variable doesn't make sense as a comparison.
            // However, [[1], [2], null] is a valid value to use in the comparison, since
            // there are definitely values that it is smaller than or bigger than.
            Ok(Type {
                base: property_type.base.clone(),
                nullable: false,
            })
        }
        Operation::Contains(..) | Operation::NotContains(..) => {
            // To be able to check whether the property's value contains the operand,
            // the property needs to be a list. If it's not a list, this is a bad filter.
            let inner_type = match &property_type.base {
                BaseType::Named(_) => {
                    return Err(FilterTypeError::ListFilterOperationOnNonListField(
                        operation.operation_name().to_string(),
                        property_name.to_string(),
                        property_type.to_string(),
                    ))
                }
                BaseType::List(inner) => inner.as_ref(),
            };

            // We're trying to see if a list of element contains our element, so its type
            // is whatever is inside the list -- nullable or not.
            Ok(inner_type.clone())
        }
        Operation::OneOf(..) | Operation::NotOneOf(..) => {
            // Whatever the property's type is, the argument must be a non-nullable list of
            // the same type, so that the elements of that list may be checked for equality
            // against that property's value.
            Ok(Type {
                base: BaseType::List(Box::new(property_type.clone())),
                nullable: false,
            })
        }
        Operation::HasPrefix(..)
        | Operation::NotHasPrefix(..)
        | Operation::HasSuffix(..)
        | Operation::NotHasSuffix(..)
        | Operation::HasSubstring(..)
        | Operation::NotHasSubstring(..)
        | Operation::RegexMatches(..)
        | Operation::NotRegexMatches(..) => {
            // Filtering operations involving strings only take non-nullable strings as inputs.
            Ok(Type {
                base: BaseType::Named(Name::new("String")),
                nullable: false,
            })
        }
        Operation::IsNull(..) | Operation::IsNotNull(..) => {
            // These are unary operations, there's no place where a variable can be used.
            // There's nothing to be inferred, and this function must never be called
            // for such operations.
            unreachable!()
        }
    }
}

fn make_filter_expr(
    schema: &Schema,
    tags: &BTreeMap<Arc<str>, ContextField>,
    current_vertex_vid: Vid,
    property_name: &Arc<str>,
    property_type: &Type,
    property_field: &FieldNode,
    filter_directive: &FilterDirective,
) -> Result<Operation<LocalField, Argument>, Vec<FrontendError>> {
    let left = LocalField {
        field_name: property_name.clone(),
        field_type: property_type.clone(),
    };

    let filter_operation = filter_directive
        .operation
        .try_map(
            move |_| Ok(left),
            |arg| {
                Ok(match arg {
                    OperatorArgument::VariableRef(var_name) => Argument::Variable(VariableRef {
                        variable_name: var_name.clone(),
                        variable_type: infer_variable_type(
                            property_name.as_ref(),
                            property_type,
                            &filter_directive.operation,
                        )?,
                    }),
                    OperatorArgument::TagRef(tag_name) => {
                        let defined_tag = tags.get(tag_name.as_ref()).ok_or_else(|| {
                            FrontendError::UndefinedTagInFilter(
                                property_name.as_ref().to_owned(),
                                tag_name.to_string(),
                            )
                        })?;

                        if defined_tag.vertex_id > current_vertex_vid {
                            return Err(FrontendError::TagUsedBeforeDefinition(
                                property_name.as_ref().to_owned(),
                                tag_name.to_string(),
                            ));
                        }
                        Argument::Tag(defined_tag.clone())
                    }
                })
            },
        )
        .map_err(|e| vec![e])?;

    // Get the tag name, if one was used.
    // The tag name is used to improve the diagnostics raised in case of bad query input.
    let maybe_tag_name = match filter_directive.operation.right() {
        Some(OperatorArgument::TagRef(tag_name)) => Some(tag_name.as_ref()),
        _ => None,
    };

    if let Err(e) = filter_operation.operand_types_valid(maybe_tag_name) {
        Err(e.into_iter().map(|x| x.into()).collect())
    } else {
        Ok(filter_operation)
    }
}

pub(crate) fn make_ir_for_query(schema: &Schema, query: &Query) -> Result<IRQuery, FrontendError> {
    validate_query_against_schema(schema, query)?;

    let mut vid_maker = successors(Some(Vid::new(NonZeroUsize::new(1).unwrap())), |x| {
        let inner_number = x.0.get();
        Some(Vid::new(
            NonZeroUsize::new(inner_number.checked_add(1).unwrap()).unwrap(),
        ))
    });
    let mut eid_maker = successors(Some(Eid::new(NonZeroUsize::new(1).unwrap())), |x| {
        let inner_number = x.0.get();
        Some(Eid::new(
            NonZeroUsize::new(inner_number.checked_add(1).unwrap()).unwrap(),
        ))
    });

    let mut errors: Vec<FrontendError> = vec![];

    let (root_field_name, root_field_pre_coercion_type, root_field_post_coercion_type, _) =
        get_field_name_and_type_from_schema(&schema.query_type.fields, &query.root_field);
    let starting_vid = vid_maker.next().unwrap();

    let root_parameters = make_edge_parameters(
        get_edge_definition_from_schema(schema, schema.query_type_name(), root_field_name.as_ref()),
        &query.root_connection.arguments,
    );

    let mut output_prefixes = Default::default();
    let mut root_component = make_query_component(
        schema,
        query,
        &mut vid_maker,
        &mut eid_maker,
        &mut output_prefixes,
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

    if errors.is_empty() {
        Ok(IRQuery {
            root_name: root_field_name.as_ref().to_owned().into(),
            root_parameters: root_parameters.unwrap(),
            root_component: root_component.into(),
            variables,
        })
    } else {
        Err(errors.into())
    }
}

fn fill_in_query_variables(
    variables: &mut BTreeMap<Arc<str>, Type>,
    component: &IRQueryComponent,
) -> Result<(), Vec<FilterTypeError>> {
    let mut errors: Vec<FilterTypeError> = vec![];

    for vertex in component.vertices.values() {
        for filter in &vertex.filters {
            if let Some(Argument::Variable(vref)) = filter.right() {
                let existing_type = variables
                    .entry(vref.variable_name.clone())
                    .or_insert_with(|| vref.variable_type.clone());

                match intersect_types(existing_type, &vref.variable_type) {
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
        }
    }

    for inner_component in component.folds.values().map(|f| f.component.as_ref()) {
        if let Err(e) = fill_in_query_variables(variables, inner_component) {
            errors.extend(e.into_iter());
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[allow(clippy::too_many_arguments)]
fn make_query_component<'a, 'schema, 'query, V, E>(
    schema: &'schema Schema,
    query: &'query Query,
    vid_maker: &mut V,
    eid_maker: &mut E,
    output_prefixes: &mut BTreeMap<Vid, (Option<Vid>, Option<&'query str>)>,
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
        (Arc<str>, &'schema Type, SmallVec<[&'query FieldNode; 1]>),
    > = Default::default();

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
        output_prefixes,
        None,
        starting_vid,
        pre_coercion_type,
        post_coercion_type,
        starting_field,
    ) {
        errors.extend(e);
    }

    // TODO: write a test case for tags that aren't used by any filter
    // TODO: write a test case for filter inside fold that uses a tag from outside the fold
    // TODO: write a test case for filter inside two nested folds that uses tag from the outer fold,
    //       and another test where it uses a tag from top-level i.e. outside either fold
    // TODO: tag processing this late is likely to cause bugs when tags are passed into folds, FIXME
    let mut maybe_duplicate_tags: Vec<(Arc<str>, ContextField)> = Default::default();
    for vid in vertices.keys() {
        let tag_iterator = property_names_by_vertex
            .get(vid)
            .into_iter()
            .flatten()
            .flat_map(|property_name| {
                let (_, property_type, property_fields) =
                    properties.get(&(*vid, property_name.clone())).unwrap();

                let name_and_type =
                    Iterator::zip(iter::repeat(property_name), iter::repeat(*property_type));

                let field_and_tag = property_fields
                    .iter()
                    .flat_map(|field| Iterator::zip(iter::repeat(field), field.tag.iter()));

                Iterator::zip(name_and_type, field_and_tag)
            });
        for ((property_name, property_type), (property_field, tag)) in tag_iterator {
            let context_field = ContextField {
                vertex_id: *vid,
                field_name: property_name.clone(),
                field_type: property_type.clone(),
            };
            let tag_name = tag
                .name
                .as_ref()
                .unwrap_or_else(|| property_field.alias.as_ref().unwrap_or(property_name))
                .clone();

            maybe_duplicate_tags.push((tag_name, context_field));
        }
    }
    let tags: BTreeMap<Arc<str>, ContextField> = match maybe_duplicate_tags
        .into_iter()
        .try_collect_unique()
        .map_err(|field_duplicates| {
            let conflict_info = DuplicatedNamesConflict {
                duplicates: field_duplicates
                    .iter()
                    .map(|(tag_name, fields)| {
                        let types_and_fields = fields
                            .iter()
                            .map(|field| {
                                let vid = field.vertex_id;
                                (
                                    vertices[&vid].0.as_ref().to_owned(),
                                    field.field_name.as_ref().to_owned(),
                                )
                            })
                            .collect();

                        (tag_name.as_ref().to_owned(), types_and_fields)
                    })
                    .collect(),
            };
            FrontendError::MultipleTagsWithSameName(conflict_info)
        }) {
        Ok(t) => t,
        Err(e) => {
            errors.push(e);
            return Err(errors);
        }
    };

    let vertex_results = vertices
        .iter()
        .map(|(vid, (uncoerced_type_name, field_node))| {
            make_vertex(
                schema,
                &property_names_by_vertex,
                &properties,
                &tags,
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

    let outputs: BTreeMap<Arc<str>, ContextField> = vertices
        .iter()
        .flat_map(|(vid, (_, vertex_field_node))| {
            vertex_field_node
                .connections
                .iter()
                .flat_map(move |(_, child_node)| {
                    child_node.output.iter().map(move |d| (*vid, child_node, d))
                })
        })
        .map(|(vid, field_node, directive)| {
            let output_name: Arc<str> = if let Some(output_name) = directive.name.as_ref() {
                output_name.clone()
            } else {
                let mut name_parts: Vec<&'query str> =
                    vec![field_node.alias.as_ref().unwrap_or(&field_node.name)];

                let mut current_vid = Some(vid);
                while let Some(v) = current_vid {
                    let (next_vid, output_prefix) = output_prefixes[&v];
                    if let Some(prefix) = output_prefix {
                        name_parts.push(prefix);
                    }
                    current_vid = next_vid;
                }
                let value: String = name_parts.iter().rev().copied().collect();
                value.into()
            };

            let (property_name, property_type, _) =
                properties.get(&(vid, field_node.name.clone())).unwrap();

            let context_field = ContextField {
                vertex_id: vid,
                field_name: property_name.clone(),
                field_type: (*property_type).clone(),
            };

            (output_name, context_field)
        })
        .try_collect_unique()
        .map_err(|field_duplicates| {
            let conflict_info = DuplicatedNamesConflict {
                duplicates: field_duplicates
                    .iter()
                    .map(|(k, fields)| {
                        let duplicate_values = fields
                            .iter()
                            .map(|field| {
                                let vid = field.vertex_id;
                                (
                                    ir_vertices[&vid].type_name.to_string(),
                                    field.field_name.to_string(),
                                )
                            })
                            .collect();
                        (k.to_string(), duplicate_values)
                    })
                    .collect(),
            };
            vec![FrontendError::MultipleOutputsWithSameName(conflict_info)]
        })?;

    Ok(IRQueryComponent {
        root: starting_vid,
        vertices: ir_vertices,
        edges: ir_edges,
        folds,
        outputs,
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
    let destination_edge = schema
        .fields
        .get(&(Arc::from(destination_type), edge_name.clone()));
    match destination_edge {
        Some(destination_edge) => {
            // The destination type has that edge too.
            let edge_type = get_underlying_named_type(&destination_edge.ty.node).as_ref();
            if edge_type == destination_type {
                // Case 4a, Ok() and no coercion required.
                Ok(None)
            } else {
                // Case 4b, Err() because it's not supported yet.
                Err(FrontendError::EdgeRecursionNeedingMultipleCoercions(
                    edge_name.to_string(),
                ))
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
                    Err(FrontendError::AmbiguousOriginEdgeRecursion(
                        edge_name.to_string(),
                    ))
                }
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn make_vertex<'schema, 'query>(
    schema: &'schema Schema,
    property_names_by_vertex: &BTreeMap<Vid, Vec<Arc<str>>>,
    properties: &BTreeMap<
        (Vid, Arc<str>),
        (Arc<str>, &'schema Type, SmallVec<[&'query FieldNode; 1]>),
    >,
    tags: &BTreeMap<Arc<str>, ContextField>,
    vid: Vid,
    uncoerced_type_name: &Arc<str>,
    field_node: &'query FieldNode,
) -> Result<IRVertex, Vec<FrontendError>> {
    let mut errors: Vec<FrontendError> = vec![];

    if !field_node.output.is_empty() {
        errors.push(FrontendError::UnsupportedEdgeOutput(
            field_node.name.as_ref().to_owned(),
        ));
    }
    if let Some(first_filter) = field_node.filter.first() {
        // TODO: If @filter on edges is allowed, tweak this.
        errors.push(FrontendError::UnsupportedEdgeFilter(
            field_node.name.as_ref().to_owned(),
        ));
    }

    let (type_name, coerced_from_type) = match field_node.coerced_to.clone().map_or_else(
        || {
            Result::<(Arc<str>, Option<Arc<str>>), FrontendError>::Ok((
                uncoerced_type_name.clone(),
                None,
            ))
        },
        |coerced_to_type| {
            let coerced_type =
                get_vertex_type_definition_from_schema(schema, coerced_to_type.as_ref())?;
            Ok((
                coerced_type.name.node.as_ref().to_owned().into(),
                Some(uncoerced_type_name.clone()),
            ))
        },
    ) {
        Ok(x) => x,
        Err(e) => {
            errors.push(e);
            return Err(errors);
        }
    };

    let mut filters = vec![];
    for property_name in property_names_by_vertex.get(&vid).into_iter().flatten() {
        let (_, property_type, property_fields) =
            properties.get(&(vid, property_name.clone())).unwrap();

        for property_field in property_fields.iter() {
            for filter_directive in property_field.filter.iter() {
                match make_filter_expr(
                    schema,
                    tags,
                    vid,
                    property_name,
                    property_type,
                    property_field,
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
        }
    }

    if errors.is_empty() {
        Ok(IRVertex {
            vid,
            type_name,
            coerced_from_type,
            filters,
        })
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
    properties: &mut BTreeMap<
        (Vid, Arc<str>),
        (Arc<str>, &'schema Type, SmallVec<[&'query FieldNode; 1]>),
    >,
    output_prefixes: &mut BTreeMap<Vid, (Option<Vid>, Option<&'query str>)>,
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

    vertices
        .try_insert(current_vid, (pre_coercion_type, current_field))
        .unwrap();

    output_prefixes
        .try_insert(
            current_vid,
            (parent_vid, current_field.alias.as_ref().map(|x| x.as_ref())),
        )
        .unwrap();

    let defined_fields = get_vertex_field_definitions(schema, post_coercion_type.as_ref());

    for (connection, subfield) in &current_field.connections {
        let (
            subfield_name,
            subfield_pre_coercion_type,
            subfield_post_coercion_type,
            subfield_raw_type,
        ) = get_field_name_and_type_from_schema(defined_fields, subfield);

        if schema
            .vertex_types
            .contains_key(subfield_post_coercion_type.as_ref())
        {
            let next_vid = vid_maker.next().unwrap();
            let next_eid = eid_maker.next().unwrap();

            if let Some(FoldDirective {}) = connection.fold {
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
                            output_prefixes,
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
                    .try_insert(next_eid, (current_vid, next_vid, connection))
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
                    output_prefixes,
                    Some(current_vid),
                    next_vid,
                    subfield_pre_coercion_type.clone(),
                    subfield_post_coercion_type.clone(),
                    subfield,
                ) {
                    errors.extend(e);
                }
            }
        } else if BUILTIN_SCALARS.contains(subfield_post_coercion_type.as_ref())
            || schema
                .scalars
                .contains_key(subfield_post_coercion_type.as_ref())
        {
            let subfield_name: Arc<str> = subfield_name.as_ref().to_owned().into();
            let key = (current_vid, subfield_name.clone());
            properties
                .entry(key)
                .and_modify(|(prior_name, prior_type, subfields)| {
                    assert_eq!(subfield_name.as_ref(), prior_name.as_ref());
                    assert_eq!(&subfield_raw_type, prior_type);
                    subfields.push(subfield);
                })
                .or_insert_with(|| {
                    property_names_by_vertex
                        .entry(current_vid)
                        .or_default()
                        .push(subfield_name.clone());

                    (subfield_name, subfield_raw_type, SmallVec::from([subfield]))
                });
        } else {
            unreachable!();
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[allow(clippy::too_many_arguments)]
fn make_fold<'a, 'schema, 'query, V, E>(
    schema: &'schema Schema,
    query: &'query Query,
    vid_maker: &mut V,
    eid_maker: &mut E,
    output_prefixes: &mut BTreeMap<Vid, (Option<Vid>, Option<&'query str>)>,
    fold_eid: Eid,
    edge_name: Arc<str>,
    edge_parameters: Option<Arc<EdgeParameters>>,
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
    let component = make_query_component(
        schema,
        query,
        vid_maker,
        eid_maker,
        output_prefixes,
        Some(parent_vid),
        starting_vid,
        starting_pre_coercion_type,
        starting_post_coercion_type,
        starting_field,
    )?;

    // TODO: properly load fold post-filters and fold-specific outputs
    let post_filters = Arc::new(vec![]);
    let fold_specific_outputs = BTreeMap::new();

    Ok(IRFold {
        eid: fold_eid,
        from_vid: parent_vid,
        to_vid: starting_vid,
        edge_name,
        parameters: edge_parameters,
        component: component.into(),
        post_filters,
        fold_specific_outputs,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use filetests_proc_macro::parameterize;

    use crate::{
        frontend::make_ir_for_query,
        schema::Schema,
        util::{TestIRQuery, TestIRQueryResult, TestParsedGraphQLQueryResult},
    };

    lazy_static! {
        static ref FILESYSTEM_SCHEMA: Schema =
            Schema::parse(fs::read_to_string("src/resources/schemas/filesystem.graphql").unwrap())
                .unwrap();
        static ref NUMBERS_SCHEMA: Schema =
            Schema::parse(fs::read_to_string("src/resources/schemas/numbers.graphql").unwrap())
                .unwrap();
        static ref NULLABLES_SCHEMA: Schema =
            Schema::parse(fs::read_to_string("src/resources/schemas/nullables.graphql").unwrap())
                .unwrap();
        static ref RECURSES_SCHEMA: Schema =
            Schema::parse(fs::read_to_string("src/resources/schemas/recurses.graphql").unwrap())
                .unwrap();
    }

    #[test]
    fn test_schemas_load_correctly() {
        // We want to merely touch the lazy-static variables so they get initialized.
        // If that succeeds, even very cursory checks will suffice.
        assert!(FILESYSTEM_SCHEMA.vertex_types.len() > 3);
        assert!(!NUMBERS_SCHEMA.vertex_types.is_empty());
        assert!(!NULLABLES_SCHEMA.vertex_types.is_empty());
        assert!(!RECURSES_SCHEMA.vertex_types.is_empty());
    }

    #[parameterize("trustfall_core/src/resources/test_data/frontend_errors")]
    fn frontend_errors(base: &Path, stem: &str) {
        parameterizable_tester(base, stem, ".frontend-error.ron")
    }

    #[parameterize("trustfall_core/src/resources/test_data/execution_errors")]
    fn execution_errors(base: &Path, stem: &str) {
        parameterizable_tester(base, stem, ".ir.ron")
    }

    #[parameterize("trustfall_core/src/resources/test_data/valid_queries")]
    fn valid_queries(base: &Path, stem: &str) {
        parameterizable_tester(base, stem, ".ir.ron")
    }

    fn parameterizable_tester(base: &Path, stem: &str, check_file_suffix: &str) {
        let mut input_path = PathBuf::from(base);
        input_path.push(format!("{}.graphql-parsed.ron", stem));

        let input_data = fs::read_to_string(input_path).unwrap();
        let test_query: TestParsedGraphQLQueryResult = ron::from_str(&input_data).unwrap();
        if test_query.is_err() {
            return;
        }
        let test_query = test_query.unwrap();

        let schema: &Schema = match test_query.schema_name.as_str() {
            "filesystem" => &FILESYSTEM_SCHEMA,
            "numbers" => &NUMBERS_SCHEMA,
            "nullables" => &NULLABLES_SCHEMA,
            "recurses" => &RECURSES_SCHEMA,
            _ => unimplemented!("unrecognized schema name: {:?}", test_query.schema_name),
        };

        let mut check_path = PathBuf::from(base);
        check_path.push(format!("{}{}", stem, check_file_suffix));
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
