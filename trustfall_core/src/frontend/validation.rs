use std::sync::Arc;

use async_graphql_parser::types::TypeKind;

use crate::{
    graphql_query::query::{FieldConnection, FieldNode, Query},
    ir::TYPENAME_META_FIELD,
    schema::Schema,
};

use super::{
    error::{FrontendError, ValidationError},
    util::get_underlying_named_type,
};

pub(super) fn validate_query_against_schema(
    schema: &Schema,
    query: &Query,
) -> Result<(), FrontendError> {
    let mut path = vec![];
    validate_field(
        schema,
        schema.query_type_name(),
        &mut path,
        &query.root_connection,
        &query.root_field,
    )
}

fn validate_field<'a>(
    schema: &Schema,
    parent_type_name: &str,
    path: &mut Vec<&'a str>,
    connection: &FieldConnection,
    node: &'a FieldNode,
) -> Result<(), FrontendError> {
    // TODO: Maybe consider a better representation that doesn't have this duplication?
    assert_eq!(connection.name, node.name);
    assert_eq!(connection.alias, node.alias);

    if node.name.as_ref() == TYPENAME_META_FIELD {
        // This is a meta field of scalar "String!" type that is guaranteed to exist.
        // We just have to make sure that it's used as a property, and not as an edge.
        if !node.connections.is_empty() {
            return Err(FrontendError::PropertyMetaFieldUsedAsEdge(
                TYPENAME_META_FIELD.to_string(),
            ));
        }

        return Ok(());
    }

    let old_path_length = path.len();
    let field_def = schema
        .fields
        .get(&(
            Arc::from(parent_type_name.to_string()),
            Arc::from(node.name.to_string()),
        ))
        .ok_or_else(|| {
            path.push(&node.name);
            FrontendError::ValidationError(ValidationError::NonExistentPath(
                path.iter().map(|x| x.to_string()).collect(),
            ))
        })?;

    path.push(&node.name);

    let pre_coercion_type_name = get_underlying_named_type(&field_def.ty.node).as_ref();
    let field_type_name = if let Some(coerced) = &node.coerced_to {
        let pre_coercion_type_definition = &schema.vertex_types[pre_coercion_type_name];
        if let TypeKind::Interface(_) = &pre_coercion_type_definition.kind {
        } else {
            // Only interface types may be coerced into other types. This is not an interface.
            return Err(FrontendError::ValidationError(
                ValidationError::CannotCoerceNonInterfaceType(
                    pre_coercion_type_name.to_string(),
                    coerced.to_string(),
                ),
            ));
        }

        if let Some(post_coercion_type_definition) = schema.vertex_types.get(coerced) {
            let implemented_interfaces = match &post_coercion_type_definition.kind {
                TypeKind::Object(o) => &o.implements,
                TypeKind::Interface(i) => &i.implements,
                TypeKind::Scalar
                | TypeKind::Union(_)
                | TypeKind::Enum(_)
                | TypeKind::InputObject(_) => unreachable!(),
            };
            if !implemented_interfaces
                .iter()
                .any(|x| x.node.as_ref() == pre_coercion_type_name)
            {
                // The specified coerced-to type does not implement the source interface.
                return Err(FrontendError::ValidationError(
                    ValidationError::CannotCoerceToUnrelatedType(
                        pre_coercion_type_name.to_string(),
                        coerced.to_string(),
                    ),
                ));
            }
        } else {
            // The coerced-to type is not part of the schema.
            return Err(FrontendError::ValidationError(
                ValidationError::NonExistentType(coerced.to_string()),
            ));
        }

        path.push(coerced);
        coerced.as_ref()
    } else {
        pre_coercion_type_name
    };

    for (child_connection, child_node) in node.connections.iter() {
        validate_field(schema, field_type_name, path, child_connection, child_node)?;
    }

    path.pop().unwrap();
    if node.coerced_to.is_some() {
        path.pop().unwrap();
    }
    assert_eq!(old_path_length, path.len());

    Ok(())
}
