//! Test support for driving the stream kernel with existing synchronous fixtures.

use std::sync::Arc;

use futures_util::{StreamExt as _, stream};

use crate::ir::{EdgeParameters, FieldValue};

use super::{
    Adapter, AsVertex, ContextIterator, ResolveEdgeInfo, ResolveInfo,
    async_adapter::{
        AsyncAdapter, ContextOutcomeStream, ContextStream, NeighborResolutionStream, VertexStream,
    },
};

/// Test-only projection of a synchronous adapter into the async protocol.
///
/// Each input context is pulled from the async stream, resolved through the sync adapter with a
/// single-item iterator, and yielded before the next context is taken. The bridge never
/// `collect()`s the full input batch, so:
///
/// - partial consumption / backpressure works,
/// - mid-stream errors surface without waiting for the rest of the batch,
/// - adapters that assume large batches still see correct 1:1 pairing (batch size 1).
///
pub(crate) struct SyncToAsyncAdapter<A> {
    inner: Arc<A>,
}

impl<A> SyncToAsyncAdapter<A> {
    /// Wrap a sync adapter (already behind [`Arc`]) as a streaming async adapter.
    pub(crate) fn new(inner: Arc<A>) -> Self {
        Self { inner }
    }

    /// Access the underlying sync adapter.
    pub(crate) fn inner(&self) -> &Arc<A> {
        &self.inner
    }
}

impl<A> Clone for SyncToAsyncAdapter<A> {
    fn clone(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}

impl<'vertex, A> AsyncAdapter<'vertex> for SyncToAsyncAdapter<A>
where
    A: Adapter<'vertex> + 'vertex,
{
    type Vertex = A::Vertex;
    type Error = A::Error;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexStream<'vertex, Result<Self::Vertex, Self::Error>> {
        let iter = self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info);
        Box::pin(stream::iter(iter))
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, Result<FieldValue, Self::Error>> {
        let inner = Arc::clone(&self.inner);
        let type_name = type_name.clone();
        let property_name = property_name.clone();
        let resolve_info = resolve_info.clone();
        // Stream one context at a time — no full-batch collect.
        Box::pin(async_stream::stream! {
            let mut contexts = contexts;
            while let Some(ctx) = contexts.next().await {
                let sync_iter: ContextIterator<'vertex, V> = Box::new(std::iter::once(ctx));
                let outcomes =
                    inner.resolve_property(sync_iter, &type_name, &property_name, &resolve_info);
                for item in outcomes {
                    yield item;
                }
            }
        })
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeStream<
        'vertex,
        V,
        NeighborResolutionStream<'vertex, Self::Vertex, Self::Error>,
    > {
        let inner = Arc::clone(&self.inner);
        let type_name = type_name.clone();
        let edge_name = edge_name.clone();
        let parameters = parameters.clone();
        let resolve_info = resolve_info.clone();
        Box::pin(async_stream::stream! {
            let mut contexts = contexts;
            while let Some(ctx) = contexts.next().await {
                let sync_iter: ContextIterator<'vertex, V> = Box::new(std::iter::once(ctx));
                let outcomes = inner.resolve_neighbors(
                    sync_iter, &type_name, &edge_name, &parameters, &resolve_info,
                );
                for (ctx, resolution) in outcomes {
                    let neighbors = resolution.map(|neighbors| {
                        let neighbors: VertexStream<'vertex, Result<Self::Vertex, Self::Error>> =
                            Box::pin(stream::iter(neighbors));
                        neighbors
                    });
                    yield (ctx, neighbors);
                }
            }
        })
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, Result<bool, Self::Error>> {
        let inner = Arc::clone(&self.inner);
        let type_name = type_name.clone();
        let coerce_to_type = coerce_to_type.clone();
        let resolve_info = resolve_info.clone();
        Box::pin(async_stream::stream! {
            let mut contexts = contexts;
            while let Some(ctx) = contexts.next().await {
                let sync_iter: ContextIterator<'vertex, V> = Box::new(std::iter::once(ctx));
                let outcomes =
                    inner.resolve_coercion(sync_iter, &type_name, &coerce_to_type, &resolve_info);
                for item in outcomes {
                    yield item;
                }
            }
        })
    }
}
