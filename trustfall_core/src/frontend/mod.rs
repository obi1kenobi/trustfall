#![allow(dead_code, unused_variables, unused_mut)]
use std::{
    collections::BTreeMap,
    convert::TryFrom,
    iter::{self, successors},
    num::NonZeroUsize,
    sync::Arc,
};

use async_graphql_parser::{
    types::{ExecutableDocument, FieldDefinition, Type, TypeDefinition, TypeKind},
    Positioned,
};
use async_graphql_value::Name;
use itertools::Itertools;
use smallvec::SmallVec;

use crate::{
    graphql_query::{
        directives::{FilterDirective, FoldDirective, OperatorArgument},
        query::{parse_document, FieldConnection, FieldNode, Query},
    },
    ir::{
        Argument, ContextField, EdgeParameters, Eid, FieldValue, IREdge, IRFold, IRQuery,
        IRQueryComponent, IRVertex, LocalField, Operation, VariableRef, Vid, indexed::IndexedQuery,
    },
    schema::{Schema, BUILTIN_SCALARS},
    util::TryCollectUniqueKey,
};

use self::{
    error::{DuplicatedNamesConflict, FrontendError, ValidationError},
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
            let pre_coercion_type_name: Arc<str> = get_underlying_named_type(field_raw_type).to_string().into();
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
) -> Result<Option<Arc<EdgeParameters>>, FrontendError> {
    let mut edge_arguments: BTreeMap<Arc<str>, FieldValue> = BTreeMap::new();
    for arg in &edge_definition.arguments {
        let arg_name = arg.node.name.node.as_ref();
        let variable_value = specified_arguments
            .get(arg_name)
            .cloned()
            .or_else(|| {
                arg.node
                    .default_value
                    .as_ref()
                    .map(|v| FieldValue::try_from(&v.node).unwrap())
            })
            .ok_or_else(|| {
                FrontendError::MissingRequiredEdgeParameter(
                    arg_name.to_string(),
                    edge_definition.name.node.to_string(),
                )
            })?;

        edge_arguments
            .try_insert(arg_name.to_owned().into(), variable_value)
            .unwrap(); // Duplicates should have been caught at parse time.
    }

    // TODO: add edge parameter type validation
    // TODO: add check that all supplied parameter types are actually supported by the schema,
    //       currently unrecognized args are just going to be ignored.
    if edge_arguments.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Arc::new(EdgeParameters(edge_arguments))))
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
) -> Result<Operation<LocalField, Argument>, FrontendError> {
    let left = LocalField {
        field_name: property_name.clone(),
        field_type: property_type.clone(),
    };

    filter_directive.operation.try_map(
        move |_| Ok(left),
        |arg| {
            Ok(match arg {
                OperatorArgument::VariableRef(var_name) => Argument::Variable(VariableRef {
                    variable_name: var_name.clone(),
                    variable_type: property_type.clone(),
                }),
                OperatorArgument::TagRef(tag_name) => {
                    let defined_tag = tags.get(tag_name.as_ref()).ok_or_else(|| {
                        FrontendError::UndefinedTagInFilter(
                            property_name.as_ref().to_owned(),
                            tag_name.to_string(),
                        )
                    })?;

                    if defined_tag.vertex_id > current_vertex_vid {
                        return Err("Filter cannot use tag defined later than its use".into());
                    }
                    Argument::Tag(defined_tag.clone())
                }
            })
        },
    )
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

    let (root_field_name, root_field_pre_coercion_type, root_field_post_coercion_type, _) =
        get_field_name_and_type_from_schema(&schema.query_type.fields, &query.root_field);
    let starting_vid = vid_maker.next().unwrap();

    let root_parameters = make_edge_parameters(
        get_edge_definition_from_schema(schema, schema.query_type_name(), root_field_name.as_ref()),
        &query.root_connection.arguments,
    )?;

    let mut output_prefixes = Default::default();
    let root_component = make_query_component(
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
    )?;

    Ok(IRQuery {
        root_name: root_field_name.as_ref().to_owned().into(),
        root_parameters,
        root_component: root_component.into(),
    })
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
) -> Result<IRQueryComponent, FrontendError>
where
    'schema: 'query,
    V: Iterator<Item = Vid>,
    E: Iterator<Item = Eid>,
{
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

    fill_in_vertex_data(
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
    )?;

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
    let tags: BTreeMap<Arc<str>, ContextField> = maybe_duplicate_tags
        .drain(..)
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
        })?;

    let ir_vertices: BTreeMap<Vid, IRVertex> = vertices
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
        })
        .fold_ok(btreemap! {}, |mut acc, vertex| {
            acc.try_insert(vertex.vid, vertex).unwrap();
            acc
        })?;

    let mut ir_edges: BTreeMap<Eid, Arc<IREdge>> = BTreeMap::new();
    for (eid, (from_vid, to_vid, field_connection)) in edges.iter() {
        let from_vertex_type = &ir_vertices[from_vid].type_name;
        let edge_definition = get_edge_definition_from_schema(
            schema,
            from_vertex_type.as_ref(),
            field_connection.name.as_ref(),
        );
        let edge_name = edge_definition.name.node.as_ref().to_owned().into();

        let parameters = make_edge_parameters(edge_definition, &field_connection.arguments)?;

        let optional = field_connection.optional.is_some();
        let recursive = field_connection.recurse.as_ref().map(|d| d.depth);

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
            FrontendError::MultipleOutputsWithSameName(conflict_info)
        })?;

    Ok(IRQueryComponent {
        root: starting_vid,
        vertices: ir_vertices,
        edges: ir_edges,
        folds,
        outputs,
    })
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
) -> Result<IRVertex, FrontendError> {
    if !field_node.output.is_empty() {
        return Err(FrontendError::UnsupportedEdgeOutput(
            field_node.name.as_ref().to_owned(),
        ));
    }
    if let Some(first_filter) = field_node.filter.first() {
        // TODO: If @filter on edges is allowed, tweak this.
        return Err(FrontendError::UnsupportedEdgeFilter(
            field_node.name.as_ref().to_owned(),
        ));
    }

    let (type_name, coerced_from_type) = field_node.coerced_to.clone().map_or_else(
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
    )?;

    let mut filters = vec![];
    for property_name in property_names_by_vertex.get(&vid).into_iter().flatten() {
        let (_, property_type, property_fields) =
            properties.get(&(vid, property_name.clone())).unwrap();

        for property_field in property_fields.iter() {
            for filter_directive in property_field.filter.iter() {
                let filter_operation = make_filter_expr(
                    schema,
                    tags,
                    vid,
                    property_name,
                    property_type,
                    property_field,
                    filter_directive,
                )?;
                filters.push(filter_operation);
            }
        }
    }

    Ok(IRVertex {
        vid,
        type_name,
        coerced_from_type,
        filters,
    })
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
) -> Result<(), FrontendError>
where
    'schema: 'query,
    V: Iterator<Item = Vid>,
    E: Iterator<Item = Eid>,
{
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
        let (subfield_name, subfield_pre_coercion_type, subfield_post_coercion_type, subfield_raw_type) =
            get_field_name_and_type_from_schema(defined_fields, subfield);

        if schema
            .vertex_types
            .contains_key(subfield_post_coercion_type.as_ref())
        {
            let next_vid = vid_maker.next().unwrap();
            let next_eid = eid_maker.next().unwrap();

            if let Some(FoldDirective {}) = connection.fold {
                if connection.optional.is_some() {
                    return Err(FrontendError::UnsupportedDirectiveOnFoldedEdge(
                        subfield.name.to_string(),
                        "@optional".to_owned(),
                    ));
                }
                if connection.recurse.is_some() {
                    return Err(FrontendError::UnsupportedDirectiveOnFoldedEdge(
                        subfield.name.to_string(),
                        "@recurse".to_owned(),
                    ));
                }

                let edge_definition = get_edge_definition_from_schema(
                    schema,
                    post_coercion_type.as_ref(),
                    connection.name.as_ref(),
                );
                let edge_parameters = make_edge_parameters(edge_definition, &connection.arguments)?;

                let fold = make_fold(
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
                )?;
                folds.insert(next_eid, fold.into());
            } else {
                edges
                    .try_insert(next_eid, (current_vid, next_vid, connection))
                    .expect("Unexpectedly encountered duplicate eid");

                fill_in_vertex_data(
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
                )?;
            }
        } else if BUILTIN_SCALARS.contains(subfield_post_coercion_type.as_ref())
            || schema.scalars.contains_key(subfield_post_coercion_type.as_ref())
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

    Ok(())
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
) -> Result<IRFold, FrontendError>
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

    // TODO: properly load fold post-filters
    let post_filters = Arc::new(vec![]);

    Ok(IRFold {
        eid: fold_eid,
        from_vid: parent_vid,
        to_vid: starting_vid,
        edge_name,
        parameters: edge_parameters,
        component: component.into(),
        post_filters,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use async_graphql_parser::parse_schema;
    use filetests_proc_macro::parameterize;

    use crate::{
        frontend::make_ir_for_query,
        schema::Schema,
        util::{TestIRQuery, TestIRQueryResult, TestParsedGraphQLQueryResult},
    };

    lazy_static! {
        static ref FILESYSTEM_SCHEMA: Schema = Schema::new(
            parse_schema(fs::read_to_string("src/resources/schemas/filesystem.graphql").unwrap())
                .unwrap()
        );
        static ref NUMBERS_SCHEMA: Schema = Schema::new(
            parse_schema(fs::read_to_string("src/resources/schemas/numbers.graphql").unwrap())
                .unwrap()
        );
    }

    #[test]
    fn test_schemas_load_correctly() {
        // We want to merely touch the lazy-static variables so they get initialized.
        // If that succeeds, even very cursory checks will suffice.
        assert!(FILESYSTEM_SCHEMA.vertex_types.len() > 3);
        assert!(!NUMBERS_SCHEMA.vertex_types.is_empty());
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
