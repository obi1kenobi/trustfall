use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;

use crate::{
    interpreter::InterpretedQuery,
    ir::{indexed::Output, Eid, FieldValue, IRQuery, Vid},
};

/// Information about the query being processed.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct QueryInfo {
    query: InterpretedQuery,
    pub(crate) current_vertex: Vid,
    pub(crate) crossing_eid: Option<Eid>,
}

impl QueryInfo {
    pub(crate) fn new(
        query: InterpretedQuery,
        current_vertex: Vid,
        crossing_eid: Option<Eid>,
    ) -> Self {
        Self {
            query,
            current_vertex,
            crossing_eid,
        }
    }

    pub(crate) fn ir_query(&self) -> &IRQuery {
        &self.query.indexed_query.ir_query
    }

    pub(crate) fn outputs(&self) -> &BTreeMap<Arc<str>, Output> {
        &self.query.indexed_query.outputs
    }

    pub(crate) fn arguments(&self) -> &Arc<BTreeMap<Arc<str>, FieldValue>> {
        &self.query.arguments
    }

    /// The unique ID of the vertex at the query location where this [`QueryInfo`] was provided.
    pub fn origin_vid(&self) -> Vid {
        self.current_vertex
    }

    /// If the query location of this [`QueryInfo`] was at an edge, this is the edge's unique ID.
    pub fn origin_crossing_eid(&self) -> Option<Eid> {
        self.crossing_eid
    }
}
