use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    interpreter::{
        Adapter, ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, ResolveInfo,
        VertexIterator,
    },
    ir::{EdgeParameters, FieldValue},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NullablesVertex;

#[derive(Debug, Clone)]
pub(crate) struct NullablesAdapter;

#[allow(unused_variables)]
impl Adapter<'static> for NullablesAdapter {
    type Vertex = NullablesVertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexIterator<'static, Self::Vertex> {
        unimplemented!()
    }

    fn resolve_property(
        &self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
        unimplemented!()
    }

    fn resolve_neighbors(
        &self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>> {
        unimplemented!()
    }

    fn resolve_coercion(
        &self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
        unimplemented!()
    }
}
