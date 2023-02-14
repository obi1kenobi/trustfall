use std::{collections::BTreeMap, sync::Arc};

use crate::ir::{Eid, FieldValue, IRQuery, Vid};

use super::InterpretedQuery;

#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct QueryInfo {
    query: InterpretedQuery,
    current_vertex: Vid,
    crossing_eid: Option<Eid>,
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

    #[allow(dead_code)]
    pub(crate) fn ir_query(&self) -> &IRQuery {
        &self.query.indexed_query.ir_query
    }

    #[allow(dead_code)]
    pub(crate) fn arguments(&self) -> &Arc<BTreeMap<Arc<str>, FieldValue>> {
        &self.query.arguments
    }

    pub fn origin_vid(&self) -> Vid {
        self.current_vertex
    }

    pub fn origin_crossing_eid(&self) -> Option<Eid> {
        self.crossing_eid
    }
}
