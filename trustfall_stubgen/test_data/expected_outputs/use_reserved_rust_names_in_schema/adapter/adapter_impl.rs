use std::sync::{Arc, OnceLock};

use trustfall::{FieldValue, Schema, provider::{AsVertex, ContextIterator, ContextOutcomeIterator, EdgeParameters, ResolveEdgeInfo, ResolveInfo, Typename, VertexIterator, resolve_coercion_using_schema, resolve_property_with}};

use super::vertex::Vertex;

static SCHEMA: OnceLock<Schema> = OnceLock::new();

#[non_exhaustive]
#[derive(Debug)]
pub struct Adapter {}

impl Adapter {
    pub const SCHEMA_TEXT: &'static str = include_str!("./schema.graphql");

    pub fn schema() -> &'static Schema {
        SCHEMA
            .get_or_init(|| {
                Schema::parse(Self::SCHEMA_TEXT).expect("not a valid schema")
            })
    }

    pub fn new() -> Self {
        Self {}
    }
}

impl<'a> trustfall::provider::Adapter<'a> for Adapter {
    type Vertex = Vertex;
    type Error = std::convert::Infallible;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexIterator<'a, Result<Self::Vertex, Self::Error>> {
        match edge_name.as_ref() {
            "const" => Box::new(super::entrypoints::const_(resolve_info).map(Ok)),
            "const2" => Box::new(super::entrypoints::const2(resolve_info).map(Ok)),
            "continue" => Box::new(super::entrypoints::continue_(resolve_info).map(Ok)),
            "continue2" => Box::new(super::entrypoints::continue2(resolve_info).map(Ok)),
            "dyn" => Box::new(super::entrypoints::dyn_(resolve_info).map(Ok)),
            "dyn2" => Box::new(super::entrypoints::dyn2(resolve_info).map(Ok)),
            "if" => Box::new(super::entrypoints::if_(resolve_info).map(Ok)),
            "if2" => Box::new(super::entrypoints::if2(resolve_info).map(Ok)),
            "mod" => Box::new(super::entrypoints::mod_(resolve_info).map(Ok)),
            "mod2" => Box::new(super::entrypoints::mod2(resolve_info).map(Ok)),
            "self" => Box::new(super::entrypoints::self_(resolve_info).map(Ok)),
            "self2" => Box::new(super::entrypoints::self2(resolve_info).map(Ok)),
            "type" => Box::new(super::entrypoints::type_(resolve_info).map(Ok)),
            "type2" => Box::new(super::entrypoints::type2(resolve_info).map(Ok)),
            "unsafe" => Box::new(super::entrypoints::unsafe_(resolve_info).map(Ok)),
            "unsafe2" => Box::new(super::entrypoints::unsafe2(resolve_info).map(Ok)),
            "where" => Box::new(super::entrypoints::where_(resolve_info).map(Ok)),
            "where2" => Box::new(super::entrypoints::where2(resolve_info).map(Ok)),
            _ => {
                unreachable!(
                    "attempted to resolve starting vertices for unexpected edge name: {edge_name}"
                )
            }
        }
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, Result<FieldValue, Self::Error>> {
        if property_name.as_ref() == "__typename" {
            return Box::new(
                resolve_property_with(contexts, |vertex| vertex.typename().into())
                    .map(|(ctx, v)| (ctx, Ok(v))),
            );
        }
        match type_name.as_ref() {
            "const2" => {
                Box::new(
                    super::properties::resolve_const2_property(
                            contexts,
                            property_name.as_ref(),
                            resolve_info,
                        )
                        .map(|(ctx, v)| (ctx, Ok(v))),
                )
            }
            "continue2" => {
                Box::new(
                    super::properties::resolve_continue2_property(
                            contexts,
                            property_name.as_ref(),
                            resolve_info,
                        )
                        .map(|(ctx, v)| (ctx, Ok(v))),
                )
            }
            "dyn2" => {
                Box::new(
                    super::properties::resolve_dyn2_property(
                            contexts,
                            property_name.as_ref(),
                            resolve_info,
                        )
                        .map(|(ctx, v)| (ctx, Ok(v))),
                )
            }
            "if2" => {
                Box::new(
                    super::properties::resolve_if2_property(
                            contexts,
                            property_name.as_ref(),
                            resolve_info,
                        )
                        .map(|(ctx, v)| (ctx, Ok(v))),
                )
            }
            "mod2" => {
                Box::new(
                    super::properties::resolve_mod2_property(
                            contexts,
                            property_name.as_ref(),
                            resolve_info,
                        )
                        .map(|(ctx, v)| (ctx, Ok(v))),
                )
            }
            "self2" => {
                Box::new(
                    super::properties::resolve_self2_property(
                            contexts,
                            property_name.as_ref(),
                            resolve_info,
                        )
                        .map(|(ctx, v)| (ctx, Ok(v))),
                )
            }
            "type2" => {
                Box::new(
                    super::properties::resolve_type2_property(
                            contexts,
                            property_name.as_ref(),
                            resolve_info,
                        )
                        .map(|(ctx, v)| (ctx, Ok(v))),
                )
            }
            "unsafe2" => {
                Box::new(
                    super::properties::resolve_unsafe2_property(
                            contexts,
                            property_name.as_ref(),
                            resolve_info,
                        )
                        .map(|(ctx, v)| (ctx, Ok(v))),
                )
            }
            "where2" => {
                Box::new(
                    super::properties::resolve_where2_property(
                            contexts,
                            property_name.as_ref(),
                            resolve_info,
                        )
                        .map(|(ctx, v)| (ctx, Ok(v))),
                )
            }
            _ => {
                unreachable!(
                    "attempted to read property '{property_name}' on unexpected type: {type_name}"
                )
            }
        }
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<
        'a,
        V,
        VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
    > {
        match type_name.as_ref() {
            "const" => {
                Box::new(
                    super::edges::resolve_const_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|(ctx, neighbors)| (
                            ctx,
                            Box::new(neighbors.map(Ok))
                                as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                        )),
                )
            }
            "continue" => {
                Box::new(
                    super::edges::resolve_continue_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|(ctx, neighbors)| (
                            ctx,
                            Box::new(neighbors.map(Ok))
                                as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                        )),
                )
            }
            "dyn" => {
                Box::new(
                    super::edges::resolve_dyn_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|(ctx, neighbors)| (
                            ctx,
                            Box::new(neighbors.map(Ok))
                                as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                        )),
                )
            }
            "if" => {
                Box::new(
                    super::edges::resolve_if_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|(ctx, neighbors)| (
                            ctx,
                            Box::new(neighbors.map(Ok))
                                as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                        )),
                )
            }
            "mod" => {
                Box::new(
                    super::edges::resolve_mod_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|(ctx, neighbors)| (
                            ctx,
                            Box::new(neighbors.map(Ok))
                                as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                        )),
                )
            }
            "self" => {
                Box::new(
                    super::edges::resolve_self_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|(ctx, neighbors)| (
                            ctx,
                            Box::new(neighbors.map(Ok))
                                as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                        )),
                )
            }
            "type" => {
                Box::new(
                    super::edges::resolve_type_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|(ctx, neighbors)| (
                            ctx,
                            Box::new(neighbors.map(Ok))
                                as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                        )),
                )
            }
            "unsafe" => {
                Box::new(
                    super::edges::resolve_unsafe_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|(ctx, neighbors)| (
                            ctx,
                            Box::new(neighbors.map(Ok))
                                as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                        )),
                )
            }
            "where" => {
                Box::new(
                    super::edges::resolve_where_edge(
                            contexts,
                            edge_name.as_ref(),
                            parameters,
                            resolve_info,
                        )
                        .map(|(ctx, neighbors)| (
                            ctx,
                            Box::new(neighbors.map(Ok))
                                as VertexIterator<'a, Result<Self::Vertex, Self::Error>>,
                        )),
                )
            }
            _ => {
                unreachable!(
                    "attempted to resolve edge '{edge_name}' on unexpected type: {type_name}"
                )
            }
        }
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        _type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, Result<bool, Self::Error>> {
        Box::new(
            resolve_coercion_using_schema(
                    contexts,
                    Self::schema(),
                    coerce_to_type.as_ref(),
                )
                .map(|(ctx, v)| (ctx, Ok(v))),
        )
    }
}
