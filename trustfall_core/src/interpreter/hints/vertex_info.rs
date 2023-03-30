use std::sync::Arc;

use crate::ir::{IREdge, IRFold, IRQueryComponent, IRVertex, Vid};

use super::EdgeInfo;

/// Information about what the currently-executing query needs at a specific vertex.
#[cfg_attr(docsrs, doc(notable_trait))]
pub trait VertexInfo {
    /// The unique ID of the vertex this [`VertexInfo`] describes.
    fn vid(&self) -> Vid;

    /// The type coercion (`... on SomeType`) applied by the query at this vertex, if any.
    fn coerced_to_type(&self) -> Option<&Arc<str>>;

    /// Returns info for the first edge by the given name that is *mandatory*:
    /// this vertex must contain the edge, or its result set will be discarded.
    ///
    /// Edges marked `@optional`, `@fold`, or `@recurse` are not mandatory:
    /// - `@optional` edges that don't exist produce `null` outputs.
    /// - `@fold` edges that don't exist produce empty aggregations.
    /// - `@recurse` always starts at depth 0 (i.e. returning the *current* vertex),
    ///   so the edge is not required to exist.
    ///
    /// Just a convenience wrapper over [`VertexInfo::edges_with_name()`].
    fn first_mandatory_edge(&self, name: &str) -> Option<EdgeInfo>;

    /// Returns info for the first edge by the given name.
    ///
    /// Just a convenience wrapper over [`VertexInfo::edges_with_name()`].
    fn first_edge(&self, name: &str) -> Option<EdgeInfo>;

    /// Returns an iterator of all the edges by that name being resolved from this vertex.
    ///
    /// This is the building block of [`VertexInfo::first_edge()`] and
    /// [`VertexInfo::first_mandatory_edge()`].
    /// When possible, prefer using those methods as they are much simpler to understand.
    fn edges_with_name<'a>(&'a self, name: &'a str) -> Box<dyn Iterator<Item = EdgeInfo> + 'a>;
}

pub(super) trait InternalVertexInfo {
    fn current_vertex(&self) -> &IRVertex;

    fn current_component(&self) -> &IRQueryComponent;

    fn make_non_folded_edge_info(&self, edge: &IREdge) -> EdgeInfo;

    fn make_folded_edge_info(&self, fold: &IRFold) -> EdgeInfo;
}

impl<T: InternalVertexInfo> VertexInfo for T {
    fn vid(&self) -> Vid {
        self.current_vertex().vid
    }

    fn coerced_to_type(&self) -> Option<&Arc<str>> {
        let vertex = self.current_vertex();
        if vertex.coerced_from_type.is_some() {
            Some(&vertex.type_name)
        } else {
            None
        }
    }

    fn edges_with_name<'a>(&'a self, name: &'a str) -> Box<dyn Iterator<Item = EdgeInfo> + 'a> {
        let component = self.current_component();
        let current_vid = self.current_vertex().vid;

        let non_folded_edges = component
            .edges
            .values()
            .filter(move |edge| edge.from_vid == current_vid && edge.edge_name.as_ref() == name)
            .map(|edge| self.make_non_folded_edge_info(edge.as_ref()));
        let folded_edges = component
            .folds
            .values()
            .filter(move |fold| fold.from_vid == current_vid && fold.edge_name.as_ref() == name)
            .map(|fold| self.make_folded_edge_info(fold.as_ref()));

        Box::new(non_folded_edges.chain(folded_edges))
    }

    fn first_mandatory_edge(&self, name: &str) -> Option<EdgeInfo> {
        self.edges_with_name(name)
            .find(|edge| !edge.folded && !edge.optional && edge.recursive.is_none())
    }

    fn first_edge(&self, name: &str) -> Option<EdgeInfo> {
        self.edges_with_name(name).next()
    }
}
