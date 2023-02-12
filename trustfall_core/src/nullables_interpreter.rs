use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    interpreter::{
        Adapter, ContextIterator, ContextOutcomeIterator, InterpretedQuery, VertexIterator,
    },
    ir::{EdgeParameters, Eid, FieldValue, Vid},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NullablesToken;

#[derive(Debug, Clone)]
pub(crate) struct NullablesAdapter;

#[allow(unused_variables)]
impl Adapter<'static> for NullablesAdapter {
    type Vertex = NullablesToken;

    fn resolve_starting_vertices(
        &mut self,
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> VertexIterator<'static, Self::Vertex> {
        unimplemented!()
    }

    fn resolve_property(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: Arc<str>,
        field_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
        unimplemented!()
    }

    fn resolve_neighbors(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: Arc<str>,
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
        edge_hint: Eid,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>> {
        unimplemented!()
    }

    fn resolve_coercion(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
        unimplemented!()
    }
}
