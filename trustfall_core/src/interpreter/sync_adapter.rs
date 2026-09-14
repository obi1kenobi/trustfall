//! Projection of a synchronous [`Adapter`] onto the stream kernel.
//!
//! This is deliberately private. It relies on a strong invariant established by
//! [`interpret_ir`](super::execution::interpret_ir): every stream in the pipeline is
//! synchronously ready. In return, it can hand each resolver the entire lazy context
//! batch without collecting it or reducing it to one-item calls.

use std::{
    fmt,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures_core::Stream;
use futures_util::{stream, task::noop_waker_ref};

use crate::ir::{EdgeParameters, FieldValue};

use super::{
    Adapter, AsVertex, ContextIterator, ResolveEdgeInfo, ResolveInfo,
    async_adapter::{AsyncAdapter, ContextOutcomeStream, ContextStream, VertexStream},
};

/// An iterator view of a stream that is guaranteed never to suspend.
///
/// A `Pending` result is a bug in the synchronous frontend, not a reason to spin or
/// block: Trustfall's synchronous API does not own an executor and must remain
/// runtime-independent.
pub(super) struct ReadyIterator<'a, T> {
    inner: Pin<Box<dyn Stream<Item = T> + 'a>>,
}

impl<'a, T> ReadyIterator<'a, T> {
    pub(super) fn new(inner: Pin<Box<dyn Stream<Item = T> + 'a>>) -> Self {
        Self { inner }
    }
}

impl<T> fmt::Debug for ReadyIterator<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadyIterator").finish_non_exhaustive()
    }
}

impl<T> Iterator for ReadyIterator<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let mut context = Context::from_waker(noop_waker_ref());
        match self.inner.as_mut().poll_next(&mut context) {
            Poll::Ready(item) => item,
            Poll::Pending => panic!(
                "an asynchronous stream suspended while executing through Trustfall's synchronous API"
            ),
        }
    }
}

/// Makes a synchronous adapter usable by the shared stream execution kernel.
pub(super) struct SyncAdapter<A> {
    inner: Arc<A>,
}

impl<A> SyncAdapter<A> {
    pub(super) fn new(inner: Arc<A>) -> Self {
        Self { inner }
    }
}

impl<'vertex, A> AsyncAdapter<'vertex> for SyncAdapter<A>
where
    A: Adapter<'vertex> + 'vertex,
{
    type Vertex = A::Vertex;

    /// The synchronous [`Adapter`] contract has no error channel, so the kernel's error type
    /// is uninhabited here and its `Result`s carry no runtime cost.
    type Error = std::convert::Infallible;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexStream<'vertex, Result<Self::Vertex, Self::Error>> {
        Box::pin(stream::iter(
            self.inner.resolve_starting_vertices(edge_name, parameters, resolve_info).map(Ok),
        ))
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, FieldValue, Self::Error> {
        let contexts: ContextIterator<'vertex, V> = Box::new(ReadyIterator::new(contexts));
        Box::pin(stream::iter(
            self.inner.resolve_property(contexts, type_name, property_name, resolve_info).map(Ok),
        ))
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
        VertexStream<'vertex, Result<Self::Vertex, Self::Error>>,
        Self::Error,
    > {
        let contexts: ContextIterator<'vertex, V> = Box::new(ReadyIterator::new(contexts));
        let outcomes =
            self.inner.resolve_neighbors(contexts, type_name, edge_name, parameters, resolve_info);
        Box::pin(stream::iter(outcomes.map(|(context, neighbors)| {
            let neighbors: VertexStream<'vertex, Result<Self::Vertex, Self::Error>> =
                Box::pin(stream::iter(neighbors.map(Ok)));
            Ok((context, neighbors))
        })))
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, bool, Self::Error> {
        let contexts: ContextIterator<'vertex, V> = Box::new(ReadyIterator::new(contexts));
        Box::pin(stream::iter(
            self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info).map(Ok),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
        rc::Rc,
        sync::Arc,
    };

    use crate::{
        frontend,
        interpreter::{
            AsVertex, ContextIterator, ContextOutcomeIterator, Typename, VertexIterator,
            basic_adapter::BasicAdapter, execution::interpret_ir,
        },
        ir::{EdgeParameters, FieldValue},
        schema::Schema,
    };

    #[derive(Clone, Debug)]
    struct Vertex(u8);

    impl Typename for Vertex {
        fn typename(&self) -> &'static str {
            "Item"
        }
    }

    struct TestAdapter {
        collect_batch: bool,
        batch_sizes: Rc<RefCell<Vec<usize>>>,
        contexts_pulled: Rc<Cell<usize>>,
    }

    impl<'a> BasicAdapter<'a> for TestAdapter {
        type Vertex = Vertex;

        fn resolve_starting_vertices(
            &self,
            _: &str,
            _: &EdgeParameters,
        ) -> VertexIterator<'a, Self::Vertex> {
            Box::new((0..4).map(Vertex))
        }

        fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
            &self,
            contexts: ContextIterator<'a, V>,
            _: &str,
            _: &str,
        ) -> ContextOutcomeIterator<'a, V, FieldValue> {
            let pulled = self.contexts_pulled.clone();
            let outcomes = contexts.map(move |context| {
                pulled.set(pulled.get() + 1);
                let value = FieldValue::Int64(context.active_vertex::<Vertex>().unwrap().0.into());
                (context, value)
            });

            if self.collect_batch {
                let outcomes: Vec<_> = outcomes.collect();
                self.batch_sizes.borrow_mut().push(outcomes.len());
                Box::new(outcomes.into_iter())
            } else {
                Box::new(outcomes)
            }
        }

        fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
            &self,
            _: ContextIterator<'a, V>,
            _: &str,
            _: &str,
            _: &EdgeParameters,
        ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Self::Vertex>> {
            unreachable!()
        }

        fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
            &self,
            _: ContextIterator<'a, V>,
            _: &str,
            _: &str,
        ) -> ContextOutcomeIterator<'a, V, bool> {
            unreachable!()
        }
    }

    #[allow(clippy::arc_with_non_send_sync)] // the synchronous API intentionally supports !Send adapters
    fn query(adapter: TestAdapter) -> Box<dyn Iterator<Item = BTreeMap<Arc<str>, FieldValue>>> {
        let schema = Schema::parse(
            "schema { query: RootSchemaQuery }\n\
             type RootSchemaQuery { Item: [Item!]! }\n\
             type Item { value: Int! }",
        )
        .unwrap();
        let query = frontend::parse(&schema, "{ Item { value @output } }").unwrap();
        Box::new(interpret_ir(Arc::new(adapter), query, Arc::new(BTreeMap::new())).unwrap())
    }

    #[test]
    fn synchronous_adapter_receives_the_whole_lazy_batch() {
        let batch_sizes = Rc::new(RefCell::new(Vec::new()));
        let adapter = TestAdapter {
            collect_batch: true,
            batch_sizes: batch_sizes.clone(),
            contexts_pulled: Rc::new(Cell::new(0)),
        };

        assert_eq!(query(adapter).count(), 4);
        assert_eq!(*batch_sizes.borrow(), [4]);
    }

    #[test]
    fn synchronous_results_remain_lazy() {
        let contexts_pulled = Rc::new(Cell::new(0));
        let adapter = TestAdapter {
            collect_batch: false,
            batch_sizes: Rc::new(RefCell::new(Vec::new())),
            contexts_pulled: contexts_pulled.clone(),
        };

        let mut rows = query(adapter);
        assert_eq!(contexts_pulled.get(), 0);
        assert!(rows.next().is_some());
        assert_eq!(contexts_pulled.get(), 1);
    }
}
