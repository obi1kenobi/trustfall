use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    interpreter::{Adapter, DataContext, InterpretedQuery},
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
    ) -> Box<dyn Iterator<Item = Self::Vertex>> {
        unimplemented!()
    }

    fn resolve_property(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::Vertex>>>,
        type_name: Arc<str>,
        field_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::Vertex>, FieldValue)>> {
        unimplemented!()
    }

    #[allow(clippy::type_complexity)]
    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::Vertex>>>,
        type_name: Arc<str>,
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
        edge_hint: Eid,
    ) -> Box<
        dyn Iterator<
            Item = (
                DataContext<Self::Vertex>,
                Box<dyn Iterator<Item = Self::Vertex>>,
            ),
        >,
    > {
        unimplemented!()
    }

    fn can_coerce_to_type(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::Vertex>>>,
        type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::Vertex>, bool)>> {
        unimplemented!()
    }
}
