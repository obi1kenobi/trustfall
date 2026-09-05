//! Focused tests for the public async helper contracts.

use std::{
    cell::Cell,
    convert::Infallible,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use futures_util::{StreamExt, stream};

use crate::{
    interpreter::{
        DataContext,
        async_adapter::ContextStream,
        async_helpers::{
            map_contexts_buffered, try_resolve_neighbors_with, try_resolve_property_with_concurrent,
        },
    },
    ir::FieldValue,
};

struct SuspendOnce<T> {
    value: Option<T>,
    suspended: bool,
    polls: Rc<Cell<usize>>,
}

impl<T> Unpin for SuspendOnce<T> {}

impl<T> Future for SuspendOnce<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let this = self.get_mut();
        this.polls.set(this.polls.get() + 1);
        if this.suspended {
            Poll::Ready(this.value.take().expect("future polled after completion"))
        } else {
            this.suspended = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

fn contexts<E: 'static>(values: Vec<Option<u8>>) -> ContextStream<'static, u8, E> {
    Box::pin(stream::iter(values.into_iter().map(|value| Ok(DataContext::new(value)))))
}

fn fallible_contexts(values: Vec<Result<Option<u8>, u8>>) -> ContextStream<'static, u8, u8> {
    Box::pin(stream::iter(values.into_iter().map(|result| result.map(DataContext::new))))
}

#[test]
fn buffered_helper_preserves_input_order_when_futures_suspend() {
    let polls = Rc::new(Cell::new(0));
    let tracked = Rc::clone(&polls);
    let stream = map_contexts_buffered(
        contexts::<Infallible>((0..8).map(Some).collect()),
        3,
        move |context| {
            let tracked = Rc::clone(&tracked);
            let value = context.active_vertex::<u8>().copied().expect("active vertex");
            SuspendOnce { value: Some(Ok((context, value))), suspended: false, polls: tracked }
        },
    );

    let values: Vec<_> = futures_executor::block_on(
        stream.map(|result| result.map(|(_, value)| value)).collect::<Vec<_>>(),
    )
    .into_iter()
    .collect::<Result<_, Infallible>>()
    .unwrap();
    assert_eq!(values, (0..8).collect::<Vec<_>>());
    assert_eq!(polls.get(), 16, "each future should suspend once before completing");
}

#[test]
fn buffered_helper_keeps_upstream_and_resolver_errors_in_input_order() {
    let stream = map_contexts_buffered(
        fallible_contexts(vec![Err(8), Ok(Some(1)), Ok(Some(2)), Err(9), Ok(Some(3))]),
        3,
        |context| {
            let vertex = context.active_vertex::<u8>().copied().unwrap();
            async move { if vertex == 2 { Err(7) } else { Ok((context, vertex)) } }
        },
    );

    let items: Vec<_> =
        futures_executor::block_on(stream.map(|result| result.map(|(_, value)| value)).collect());
    assert_eq!(items, vec![Err(8), Ok(1), Err(7), Err(9), Ok(3)]);
}

#[test]
fn concurrent_property_helper_preserves_nulls_and_skips_the_resolver() {
    let calls = Arc::new(AtomicUsize::new(0));
    let stream = try_resolve_property_with_concurrent(contexts(vec![Some(1), None, Some(3)]), 2, {
        let calls = Arc::clone(&calls);
        move |vertex| {
            let calls = Arc::clone(&calls);
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok::<_, Infallible>(FieldValue::Uint64(u64::from(vertex)))
            }
        }
    });

    let values: Vec<Result<FieldValue, Infallible>> =
        futures_executor::block_on(stream.map(|result| result.map(|(_, value)| value)).collect());
    assert_eq!(
        values,
        [Ok(FieldValue::Uint64(1)), Ok(FieldValue::Null), Ok(FieldValue::Uint64(3)),],
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
#[should_panic(expected = "concurrency must be at least 1")]
fn buffered_helper_rejects_zero_concurrency() {
    let _ = map_contexts_buffered(contexts::<Infallible>(vec![Some(1)]), 0, |context| async move {
        Ok((context, ()))
    });
}

#[test]
fn try_resolve_neighbors_reports_context_level_failures_in_the_outer_stream() {
    let stream = try_resolve_neighbors_with(contexts(vec![Some(1), Some(2)]), |vertex: &u8| {
        if *vertex == 1 { Ok(vec![10u8, 11]) } else { Err("immediate failure") }
    });

    let mut outcomes: Vec<Result<(Option<u8>, _), &str>> = futures_executor::block_on(
        stream
            .map(|result| {
                result.map(|(ctx, neighbors)| (ctx.active_vertex::<u8>().copied(), neighbors))
            })
            .collect::<Vec<_>>(),
    );

    let (vertex, neighbors) = outcomes.remove(0).unwrap();
    assert_eq!(vertex, Some(1));
    assert_eq!(futures_executor::block_on(neighbors.collect::<Vec<_>>()), vec![Ok(10), Ok(11)]);
    assert!(matches!(outcomes.remove(0), Err("immediate failure")));
}
