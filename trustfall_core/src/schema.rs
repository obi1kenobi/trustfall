#![allow(dead_code)]
use std::{collections::{HashMap, HashSet}, sync::Arc};

use async_graphql_parser::{types::{
    DirectiveDefinition, FieldDefinition, ObjectType, SchemaDefinition, ServiceDocument,
    TypeDefinition, TypeKind, TypeSystemDefinition,
}, parse_schema};

pub use ::async_graphql_parser::Error;

#[derive(Debug, Clone)]
pub struct Schema {
    pub(crate) schema: SchemaDefinition,
    pub(crate) query_type: ObjectType,
    pub(crate) directives: HashMap<Arc<str>, DirectiveDefinition>,
    pub(crate) scalars: HashMap<Arc<str>, TypeDefinition>,
    pub(crate) vertex_types: HashMap<Arc<str>, TypeDefinition>,
    pub(crate) fields: HashMap<(Arc<str>, Arc<str>), FieldDefinition>,
}

lazy_static! {
    pub(crate) static ref BUILTIN_SCALARS: HashSet<&'static str> = hashset! {
        "Int",
        "Float",
        "String",
        "Boolean",
        "ID",
    };
}

impl Schema {
    pub fn parse(input: impl AsRef<str>) -> Result<Self, Error> {
        parse_schema(input).map(Schema::new)
    }

    pub fn new(doc: ServiceDocument) -> Self {
        let mut schema: Option<SchemaDefinition> = None;
        let mut directives: HashMap<Arc<str>, DirectiveDefinition> =
            Default::default();
        let mut scalars: HashMap<Arc<str>, TypeDefinition> = Default::default();

        // The schema is mostly type definitions, except for one schema definition, and
        // perhaps a small number of other definitions like custom scalars or directives.
        let mut vertex_types: HashMap<Arc<str>, TypeDefinition> =
            HashMap::with_capacity(doc.definitions.len() - 1);

        // Each type has probably at least one field.
        let mut fields: HashMap<(Arc<str>, Arc<str>), FieldDefinition> =
            HashMap::with_capacity(doc.definitions.len() - 1);

        for definition in doc.definitions {
            match definition {
                TypeSystemDefinition::Schema(s) => {
                    assert!(schema.is_none());
                    if s.node.extend {
                        unimplemented!();
                    }

                    schema = Some(s.node);
                }
                TypeSystemDefinition::Directive(d) => {
                    directives
                        .try_insert(Arc::from(d.node.name.node.to_string()), d.node)
                        .unwrap();
                }
                TypeSystemDefinition::Type(t) => {
                    let node = t.node;
                    let type_name: Arc<str> = Arc::from(node.name.node.to_string());
                    assert!(!BUILTIN_SCALARS.contains(type_name.as_ref()));

                    if node.extend {
                        unimplemented!();
                    }

                    match &node.kind {
                        TypeKind::Scalar => {
                            scalars.try_insert(type_name.clone(), node.clone()).unwrap();
                        }
                        TypeKind::Object(_) | TypeKind::Interface(_) => {
                            vertex_types.try_insert(type_name.clone(), node.clone()).unwrap();
                        }
                        TypeKind::Enum(_) => unimplemented!(),
                        TypeKind::Union(_) => unimplemented!(),
                        TypeKind::InputObject(_) => unimplemented!(),
                    }

                    let field_defs = match node.kind {
                        TypeKind::Object(object) => Some(object.fields),

                        TypeKind::Interface(interface) => Some(interface.fields),
                        _ => None,
                    };
                    if let Some(field_defs) = field_defs {
                        for field in field_defs {
                            let field_node = field.node;
                            let field_name = Arc::from(field_node.name.node.to_string());

                            fields
                                .try_insert((type_name.clone(), field_name), field_node)
                                .unwrap();
                        }
                    }
                }
            }
        }

        let schema = schema.expect("Schema definition was not present.");
        let query_type_name = schema
            .query
            .as_ref()
            .expect("No query type was declared in the schema")
            .node
            .as_ref();
        let query_type_definition = vertex_types
            .get(query_type_name)
            .expect("The query type set in the schema object was never defined.");
        let query_type = match &query_type_definition.kind {
            TypeKind::Object(o) => o.clone(),
            _ => unreachable!(),
        };

        Self {
            schema,
            query_type,
            directives,
            scalars,
            vertex_types,
            fields,
        }
    }

    pub(crate) fn query_type_name(&self) -> &str {
        self.schema.query.as_ref().unwrap().node.as_ref()
    }
}
