use std::sync::Arc;

use async_graphql_parser::types::{
    BaseType, FieldDefinition, InputValueDefinition, Type, TypeDefinition, TypeKind,
};

use crate::{
    accessor_property, field_property,
    interpreter::{
        helpers::{resolve_neighbors_with, resolve_property_with},
        ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, ResolveInfo, VertexInfo,
        VertexIterator,
    },
    ir::{types::get_base_named_type, EdgeParameters, FieldValue},
};

use super::Schema;

/// A Trustfall adapter for querying Trustfall schemas.
///
/// The schema matching this adapter is in the adjacent
/// [`schema.graphql` file](https://github.com/obi1kenobi/trustfall/blob/main/trustfall_core/src/schema/adapter/schema.graphql),
/// and is also available via the [`SchemaAdapter::schema_text()`] function.
///
/// # Example
///
/// Create the adapter for querying a given schema like so:
/// ```rust
/// # use trustfall_core::schema::{Schema, SchemaAdapter};
/// #
/// # fn main() {
/// let schema_text = include_str!("./schema.graphql");
/// let schema = Schema::parse(schema_text).expect("not a valid schema");
///
/// // Create an adapter that queries
/// // the schema in the local `schema.graphql` file.
/// # [allow(unused_variables)]
/// let adapter = SchemaAdapter::new(&schema);
///
/// // Run queries using the adapter, etc.
/// # }
/// ```
///
/// Then you can query the contents of that schema.
/// For example, the following query asks for all vertex properties and their types:
/// ```graphql
/// query {
///     VertexType {
///         name @output
///
///         property {
///             property_name: name @output
///             property_type: type @output
///         }
///     }
/// }
/// ```
#[derive(Debug)]
pub struct SchemaAdapter<'a> {
    schema: &'a Schema,
}

impl<'a> SchemaAdapter<'a> {
    /// Make an adapter for querying the given Trustfall schema.
    #[inline(always)]
    pub fn new(schema_to_query: &'a Schema) -> Self {
        Self {
            schema: schema_to_query,
        }
    }

    /// A schema that describes Trustfall schemas.
    ///
    /// Queries on this adapter must conform to this schema.
    pub fn schema_text() -> &'static str {
        include_str!("./schema.graphql")
    }
}

#[derive(Debug, Clone)]
pub enum SchemaVertex<'a> {
    VertexType(VertexType<'a>),
    Property(Property<'a>),
    Edge(Edge<'a>),
    EdgeParameter(EdgeParameter<'a>),
}

impl<'a> SchemaVertex<'a> {
    #[inline(always)]
    fn as_vertex_type(&self) -> Option<&VertexType<'a>> {
        match self {
            Self::VertexType(v) => Some(v),
            _ => None,
        }
    }

    #[inline(always)]
    fn as_property(&self) -> Option<&Property<'a>> {
        match self {
            Self::Property(p) => Some(p),
            _ => None,
        }
    }

    #[inline(always)]
    fn as_edge(&self) -> Option<&Edge<'a>> {
        match self {
            Self::Edge(e) => Some(e),
            _ => None,
        }
    }

    #[inline(always)]
    fn as_edge_parameter(&self) -> Option<&EdgeParameter<'a>> {
        match self {
            Self::EdgeParameter(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VertexType<'a> {
    defn: &'a TypeDefinition,
}

impl<'a> VertexType<'a> {
    #[inline(always)]
    fn new(defn: &'a TypeDefinition) -> Self {
        Self { defn }
    }

    #[inline(always)]
    fn name(&self) -> &'a str {
        self.defn.name.node.as_str()
    }

    #[inline(always)]
    fn is_interface(&self) -> bool {
        matches!(self.defn.kind, TypeKind::Interface(..))
    }
}

#[derive(Debug, Clone)]
pub struct Property<'a> {
    parent: &'a TypeDefinition,
    name: &'a str,
    type_: &'a Type,
}

impl<'a> Property<'a> {
    #[inline(always)]
    fn new(parent: &'a TypeDefinition, name: &'a str, type_: &'a Type) -> Self {
        Self {
            parent,
            name,
            type_,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Edge<'a> {
    defn: &'a FieldDefinition,
}

impl<'a> Edge<'a> {
    #[inline(always)]
    fn new(defn: &'a FieldDefinition) -> Self {
        Self { defn }
    }

    #[inline(always)]
    fn name(&self) -> &'a str {
        &self.defn.name.node
    }

    #[inline(always)]
    fn to_many(&self) -> bool {
        matches!(self.defn.ty.node.base, BaseType::List(..))
    }

    #[inline(always)]
    fn at_least_one(&self) -> bool {
        !self.defn.ty.node.nullable
    }
}

#[derive(Debug, Clone)]
pub struct EdgeParameter<'a> {
    defn: &'a InputValueDefinition,
}

impl<'a> EdgeParameter<'a> {
    #[inline(always)]
    fn new(defn: &'a InputValueDefinition) -> Self {
        Self { defn }
    }

    #[inline(always)]
    fn name(&self) -> &'a str {
        &self.defn.name.node
    }

    #[inline(always)]
    fn type_(&self) -> String {
        self.defn.ty.node.to_string()
    }
}

impl<'a> crate::interpreter::Adapter<'a> for SchemaAdapter<'a> {
    type Vertex = SchemaVertex<'a>;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        _parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexIterator<'a, Self::Vertex> {
        let candidate_value = resolve_info.statically_required_property("name").clone();
        match edge_name.as_ref() {
            "VertexType" => {
                let root_query_type = self.schema.query_type_name();

                if let Some(crate::interpreter::CandidateValue::Single(FieldValue::String(
                    name_wanted,
                ))) = candidate_value
                {
                    let name_wanted = name_wanted.as_str();
                    if let Some(exact_wanted) = self
                        .schema
                        .vertex_types
                        .get(name_wanted)
                        .filter(move |v| v.name.node != root_query_type)
                    {
                        Box::new(std::iter::once(SchemaVertex::VertexType(VertexType::new(
                            exact_wanted,
                        ))))
                    } else {
                        Box::new(std::iter::empty())
                    }
                } else if let Some(crate::interpreter::CandidateValue::Multiple(possibilities)) =
                    candidate_value
                {
                    let possibilities_as_owned_strings = possibilities
                        .iter()
                        .map(|el| {
                            el.as_str()
                                .expect("for possibilities of names to be strings")
                                .to_string()
                        })
                        .collect::<Vec<_>>();

                    let vertex_types = &self.schema.vertex_types;

                    Box::new(
                        possibilities_as_owned_strings
                            .into_iter()
                            .filter_map(move |wanted| {
                                vertex_types
                                    .get(wanted.as_str())
                                    .filter(move |v| v.name.node != root_query_type)
                                    .map(|exact_wanted| {
                                        SchemaVertex::VertexType(VertexType::new(exact_wanted))
                                    })
                            }),
                    )
                } else {
                    Box::new(self.schema.vertex_types.values().filter_map(move |v| {
                        (v.name.node != root_query_type)
                            .then(|| SchemaVertex::VertexType(VertexType::new(v)))
                    }))
                }
            }
            "Entrypoint" => Box::new(Box::new(
                self.schema
                    .query_type
                    .fields
                    .iter()
                    .map(|field| SchemaVertex::Edge(Edge::new(&field.node))),
            )),
            _ => unreachable!("unexpected starting edge: {edge_name}"),
        }
    }

    fn resolve_property(
        &self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
        match type_name.as_ref() {
            "VertexType" => match property_name.as_ref() {
                "name" => resolve_property_with(contexts, accessor_property!(as_vertex_type, name)),
                "is_interface" => resolve_property_with(
                    contexts,
                    accessor_property!(as_vertex_type, is_interface),
                ),
                _ => unreachable!("unexpected property name on type {type_name}: {property_name}"),
            },
            "Property" => match property_name.as_ref() {
                "name" => resolve_property_with(contexts, field_property!(as_property, name)),
                "type" => resolve_property_with(
                    contexts,
                    field_property!(as_property, type_, { type_.to_string().into() }),
                ),
                _ => unreachable!("unexpected property name on type {type_name}: {property_name}"),
            },
            "Edge" => match property_name.as_ref() {
                "name" => resolve_property_with(contexts, accessor_property!(as_edge, name)),
                "to_many" => resolve_property_with(contexts, accessor_property!(as_edge, to_many)),
                "at_least_one" => {
                    resolve_property_with(contexts, accessor_property!(as_edge, at_least_one))
                }
                _ => unreachable!("unexpected property name on type {type_name}: {property_name}"),
            },
            "EdgeParameter" => match property_name.as_ref() {
                "name" => {
                    resolve_property_with(contexts, accessor_property!(as_edge_parameter, name))
                }
                "type" => {
                    resolve_property_with(contexts, accessor_property!(as_edge_parameter, type_))
                }
                _ => unreachable!("unexpected property name on type {type_name}: {property_name}"),
            },
            _ => unreachable!("unexpected type name: {type_name}"),
        }
    }

    fn resolve_neighbors(
        &self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        _parameters: &EdgeParameters,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>> {
        let schema = self.schema;
        match type_name.as_ref() {
            "VertexType" => match edge_name.as_ref() {
                "implements" => resolve_neighbors_with(contexts, move |vertex| {
                    resolve_vertex_type_implements_edge(schema, vertex)
                }),
                "implementer" => resolve_neighbors_with(contexts, move |vertex| {
                    resolve_vertex_type_implementer_edge(schema, vertex)
                }),
                "property" => resolve_neighbors_with(contexts, move |vertex| {
                    resolve_vertex_type_property_edge(schema, vertex)
                }),
                "edge" => resolve_neighbors_with(contexts, move |vertex| {
                    resolve_vertex_type_edge_edge(schema, vertex)
                }),
                _ => unreachable!("unexpected edge name on type {type_name}: {edge_name}"),
            },
            "Edge" => match edge_name.as_ref() {
                "target" => resolve_neighbors_with(contexts, move |vertex| {
                    let vertex = vertex.as_edge().expect("not an Edge");
                    let target_type = get_base_named_type(&vertex.defn.ty.node);
                    Box::new(
                        schema
                            .vertex_types
                            .get(target_type)
                            .map(|defn| SchemaVertex::VertexType(VertexType::new(defn)))
                            .into_iter(),
                    )
                }),
                "parameter" => resolve_neighbors_with(contexts, move |vertex| {
                    let vertex = vertex.as_edge().expect("not an Edge");
                    let parameters = vertex.defn.arguments.as_slice();

                    Box::new(
                        parameters
                            .iter()
                            .map(|inp| SchemaVertex::EdgeParameter(EdgeParameter::new(&inp.node))),
                    )
                }),
                _ => unreachable!("unexpected edge name on type {type_name}: {edge_name}"),
            },
            _ => unreachable!("unexpected type name: {type_name}"),
        }
    }

    #[allow(unused_variables)]
    fn resolve_coercion(
        &self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
        unreachable!("unexpected type coercion: {type_name} -> {coerce_to_type}")
    }
}

#[inline(always)]
fn resolve_vertex_type_implements_edge<'a>(
    schema: &'a Schema,
    vertex: &SchemaVertex<'a>,
) -> Box<dyn Iterator<Item = SchemaVertex<'a>> + 'a> {
    let vertex = vertex.as_vertex_type().expect("not a VertexType");
    let implements = super::get_vertex_type_implements(vertex.defn);

    Box::new(implements.iter().filter_map(move |x| {
        let implements_type = x.node.as_str();
        schema
            .vertex_types
            .get(implements_type)
            .map(|defn| SchemaVertex::VertexType(VertexType::new(defn)))
    }))
}

#[inline(always)]
fn resolve_vertex_type_implementer_edge<'a>(
    schema: &'a Schema,
    vertex: &SchemaVertex<'a>,
) -> Box<dyn Iterator<Item = SchemaVertex<'a>> + 'a> {
    let vertex = vertex.as_vertex_type().expect("not a VertexType");
    Box::new(
        schema
            .subtypes(vertex.defn.name.node.as_str())
            .expect("input type was not part of this schema")
            .filter_map(|implementer_type| {
                schema
                    .vertex_types
                    .get(implementer_type)
                    .map(|x| SchemaVertex::VertexType(VertexType::new(x)))
            }),
    )
}

#[inline(always)]
fn resolve_vertex_type_property_edge<'a>(
    schema: &'a Schema,
    vertex: &SchemaVertex<'a>,
) -> Box<dyn Iterator<Item = SchemaVertex<'a>> + 'a> {
    let vertex = vertex.as_vertex_type().expect("not a VertexType");
    let fields = super::get_vertex_type_fields(vertex.defn);

    let parent_defn = vertex.defn;
    Box::new(fields.iter().filter_map(move |p| {
        let field = &p.node;
        let field_ty = &field.ty.node;
        let base_ty = get_base_named_type(field_ty);

        if !schema.vertex_types.contains_key(base_ty) {
            Some(SchemaVertex::Property(Property::new(
                parent_defn,
                field.name.node.as_str(),
                field_ty,
            )))
        } else {
            None
        }
    }))
}

#[inline(always)]
fn resolve_vertex_type_edge_edge<'a>(
    schema: &'a Schema,
    vertex: &SchemaVertex<'a>,
) -> Box<dyn Iterator<Item = SchemaVertex<'a>> + 'a> {
    let vertex = vertex.as_vertex_type().expect("not a VertexType");
    let fields = super::get_vertex_type_fields(vertex.defn);

    Box::new(fields.iter().filter_map(move |p| {
        let field = &p.node;
        let field_ty = &field.ty.node;
        let base_ty = get_base_named_type(field_ty);

        if schema.vertex_types.contains_key(base_ty) {
            Some(SchemaVertex::Edge(Edge::new(field)))
        } else {
            None
        }
    }))
}

#[cfg(test)]
mod tests;
