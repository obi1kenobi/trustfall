//! Projection of a synchronous [`FallibleAdapter`] onto the stream kernel.
//!
//! This is deliberately private. It relies on a strong invariant established by
//! [`interpret_ir`](super::execution::interpret_ir): every stream in the pipeline is
//! synchronously ready. In return, it can hand each resolver the entire lazy context
//! batch without collecting it or reducing it to one-item calls.

use std::{
    cell::RefCell,
    collections::VecDeque,
    fmt,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures_core::Stream;
use futures_util::{stream, task::noop_waker_ref};

use crate::ir::{EdgeParameters, FieldValue};

use super::{
    AsVertex, ContextIterator, FallibleAdapter, ResolveEdgeInfo, ResolveInfo,
    async_adapter::{ContextOutcomeStream, ContextStream, FallibleAsyncAdapter, VertexStream},
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

fn separate_upstream_errors<'vertex, V, E>(
    contexts: ContextStream<'vertex, V, E>,
) -> (ContextIterator<'vertex, V>, Arc<RefCell<VecDeque<E>>>)
where
    V: 'vertex,
    E: 'vertex,
{
    let errors = Arc::new(RefCell::new(VecDeque::new()));
    let queued_errors = errors.clone();
    let mut contexts = ReadyIterator::new(contexts);
    let contexts = Box::new(std::iter::from_fn(move || {
        loop {
            match contexts.next()? {
                Ok(context) => return Some(context),
                Err(error) => queued_errors.borrow_mut().push_back(error),
            }
        }
    }));
    (contexts, errors)
}

fn restore_upstream_errors<'vertex, V, O, E>(
    errors: Arc<RefCell<VecDeque<E>>>,
    mut outcomes: impl Iterator<Item = Result<(super::DataContext<V>, O), E>> + 'vertex,
) -> ContextOutcomeStream<'vertex, V, O, E>
where
    V: 'vertex,
    O: 'vertex,
    E: 'vertex,
{
    let mut held_outcome = None;
    Box::pin(stream::iter(std::iter::from_fn(move || {
        if let Some(error) = errors.borrow_mut().pop_front() {
            return Some(Err(error));
        }

        let outcome = held_outcome.take().or_else(|| outcomes.next())?;
        if let Some(error) = errors.borrow_mut().pop_front() {
            held_outcome = Some(outcome);
            Some(Err(error))
        } else {
            Some(outcome)
        }
    })))
}

impl<'vertex, A> FallibleAsyncAdapter<'vertex> for SyncAdapter<A>
where
    A: FallibleAdapter<'vertex> + 'vertex,
{
    type Vertex = A::Vertex;
    type Error = A::Error;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexStream<'vertex, Result<Self::Vertex, Self::Error>> {
        Box::pin(stream::iter(self.inner.resolve_starting_vertices(
            edge_name,
            parameters,
            resolve_info,
        )))
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V, Self::Error>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, FieldValue, Self::Error> {
        let (contexts, errors) = separate_upstream_errors(contexts);
        restore_upstream_errors(
            errors,
            self.inner.resolve_property(contexts, type_name, property_name, resolve_info),
        )
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V, Self::Error>,
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
        let (contexts, errors) = separate_upstream_errors(contexts);
        let outcomes =
            self.inner.resolve_neighbors(contexts, type_name, edge_name, parameters, resolve_info);
        restore_upstream_errors(
            errors,
            outcomes.map(|outcome| {
                outcome.map(|(context, neighbors)| {
                    let neighbors: VertexStream<'vertex, Result<Self::Vertex, Self::Error>> =
                        Box::pin(stream::iter(neighbors));
                    (context, neighbors)
                })
            }),
        )
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V, Self::Error>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, bool, Self::Error> {
        let (contexts, errors) = separate_upstream_errors(contexts);
        restore_upstream_errors(
            errors,
            self.inner.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info),
        )
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
        let rows = interpret_ir(Arc::new(adapter), query, Arc::new(BTreeMap::new())).unwrap();
        Box::new(rows.map(|row| row.expect("BasicAdapter is infallible")))
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
