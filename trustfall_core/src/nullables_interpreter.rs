use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    interpreter::{Adapter, ContextIterator, ContextOutcomeIterator, QueryInfo, VertexIterator},
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
        &mut self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        query_info: &QueryInfo,
    ) -> VertexIterator<'static, Self::Vertex> {
        unimplemented!()
    }

    fn resolve_property(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
        unimplemented!()
    }

    fn resolve_neighbors(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>> {
        unimplemented!()
    }

    fn resolve_coercion(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
        unimplemented!()
    }
}
