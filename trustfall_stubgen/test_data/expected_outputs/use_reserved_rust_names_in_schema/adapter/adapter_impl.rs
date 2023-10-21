use std::sync::{Arc, OnceLock};

use trustfall::{FieldValue, Schema, provider::{ContextIterator, ContextOutcomeIterator, EdgeParameters, ResolveEdgeInfo, ResolveInfo, Typename, VertexIterator, resolve_coercion_using_schema, resolve_property_with}};

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

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexIterator<'a, Self::Vertex> {
        match edge_name.as_ref() {
            "const" => super::entrypoints::const_(resolve_info),
            "const2" => super::entrypoints::const2(resolve_info),
            "continue" => super::entrypoints::continue_(resolve_info),
            "continue2" => super::entrypoints::continue2(resolve_info),
            "dyn" => super::entrypoints::dyn_(resolve_info),
            "dyn2" => super::entrypoints::dyn2(resolve_info),
            "if" => super::entrypoints::if_(resolve_info),
            "if2" => super::entrypoints::if2(resolve_info),
            "mod" => super::entrypoints::mod_(resolve_info),
            "mod2" => super::entrypoints::mod2(resolve_info),
            "self" => super::entrypoints::self_(resolve_info),
            "self2" => super::entrypoints::self2(resolve_info),
            "type" => super::entrypoints::type_(resolve_info),
            "type2" => super::entrypoints::type2(resolve_info),
            "unsafe" => super::entrypoints::unsafe_(resolve_info),
            "unsafe2" => super::entrypoints::unsafe2(resolve_info),
            "where" => super::entrypoints::where_(resolve_info),
            "where2" => super::entrypoints::where2(resolve_info),
            _ => {
                unreachable!(
                    "attempted to resolve starting vertices for unexpected edge name: {edge_name}"
                )
            }
        }
    }

    fn resolve_property(
        &self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
        if property_name.as_ref() == "__typename" {
            return resolve_property_with(contexts, |vertex| vertex.typename().into());
        }
        match type_name.as_ref() {
            "const2" => {
                super::properties::resolve_const2_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "continue2" => {
                super::properties::resolve_continue2_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "dyn2" => {
                super::properties::resolve_dyn2_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "if2" => {
                super::properties::resolve_if2_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "mod2" => {
                super::properties::resolve_mod2_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "self2" => {
                super::properties::resolve_self2_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "type2" => {
                super::properties::resolve_type2_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "unsafe2" => {
                super::properties::resolve_unsafe2_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            "where2" => {
                super::properties::resolve_where2_property(
                    contexts,
                    property_name.as_ref(),
                    resolve_info,
                )
            }
            _ => {
                unreachable!(
                    "attempted to read property '{property_name}' on unexpected type: {type_name}"
                )
            }
        }
    }

    fn resolve_neighbors(
        &self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>> {
        match type_name.as_ref() {
            "const" => {
                super::edges::resolve_const_edge(
                    contexts,
                    edge_name.as_ref(),
                    parameters,
                    resolve_info,
                )
            }
            "continue" => {
                super::edges::resolve_continue_edge(
                    contexts,
                    edge_name.as_ref(),
                    parameters,
                    resolve_info,
                )
            }
            "dyn" => {
                super::edges::resolve_dyn_edge(
                    contexts,
                    edge_name.as_ref(),
                    parameters,
                    resolve_info,
                )
            }
            "if" => {
                super::edges::resolve_if_edge(
                    contexts,
                    edge_name.as_ref(),
                    parameters,
                    resolve_info,
                )
            }
            "mod" => {
                super::edges::resolve_mod_edge(
                    contexts,
                    edge_name.as_ref(),
                    parameters,
                    resolve_info,
                )
            }
            "self" => {
                super::edges::resolve_self_edge(
                    contexts,
                    edge_name.as_ref(),
                    parameters,
                    resolve_info,
                )
            }
            "type" => {
                super::edges::resolve_type_edge(
                    contexts,
                    edge_name.as_ref(),
                    parameters,
                    resolve_info,
                )
            }
            "unsafe" => {
                super::edges::resolve_unsafe_edge(
                    contexts,
                    edge_name.as_ref(),
                    parameters,
                    resolve_info,
                )
            }
            "where" => {
                super::edges::resolve_where_edge(
                    contexts,
                    edge_name.as_ref(),
                    parameters,
                    resolve_info,
                )
            }
            _ => {
                unreachable!(
                    "attempted to resolve edge '{edge_name}' on unexpected type: {type_name}"
                )
            }
        }
    }

    fn resolve_coercion(
        &self,
        contexts: ContextIterator<'a, Self::Vertex>,
        _type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
        resolve_coercion_using_schema(contexts, Self::schema(), coerce_to_type.as_ref())
    }
}
