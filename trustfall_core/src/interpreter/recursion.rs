use std::collections::VecDeque;
use std::fmt::Debug;
use std::iter::Peekable;
use std::rc::Rc;
use std::{cell::RefCell, sync::Arc};

use crate::ir::{EdgeParameters, Eid, IRQueryComponent, IRVertex, Recursive};

use super::execution::{perform_coercion};
use super::{Adapter, DataContext, InterpretedQuery};

// https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=7f560f5f73145a8a2dc2714784c208ed
// https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=c5fa6b41f1d719871d71b4a35aa9148d
// https://github.com/bojanserafimov/rust-experiments/blob/main/rec/src/main.rs

enum Next<'a, T> {
    Done(T),
    Nodes(Box<dyn Iterator<Item = T> + 'a>),
}

struct Bundle<'a, T> {
    next_: Next<'a, T>,
    is_initial: bool,
}

impl<'a, T> Bundle<'a, T> {
    fn new(next: Next<'a, T>) -> Self {
        Self {
            next_: next,
            is_initial: true,
        }
    }

    fn new_subsequent(next: Next<'a, T>) -> Self {
        Self {
            next_: next,
            is_initial: false,
        }
    }
}

type IterBundle<'a, T> = Peekable<Box<dyn Iterator<Item = Bundle<'a, T>> + 'a>>;

struct BundleMonad<'a, T: 'a> {
    inner: Box<dyn Iterator<Item = (T, Box<dyn Iterator<Item = T> + 'a>)> + 'a>,
    queue: Rc<RefCell<VecDeque<Bundle<'a, T>>>>,
}

impl<'a, T: Clone + 'a> BundleMonad<'a, T> {
    fn bind<F>(from: IterBundle<'a, T>, neighbors: &F) -> Self
    where
        F: Fn(
            Box<dyn Iterator<Item = T> + 'a>,
        ) -> Box<dyn Iterator<Item = (T, Box<dyn Iterator<Item = T> + 'a>)> + 'a>,
    {
        let queue = Rc::new(RefCell::new(VecDeque::new()));
        let queue_clone = Rc::clone(&queue);
        let flattened = Box::new(from.flat_map(move |bundle| match bundle.next_ {
            Next::Done(x) => {
                queue
                    .borrow_mut()
                    .push_back(Bundle::new_subsequent(Next::Done(x)));
                let iter: Box<dyn Iterator<Item = T> + 'a> = Box::new(vec![].into_iter());
                iter
            }
            Next::Nodes(nodes) => {
                let queue_clone = Rc::clone(&queue);
                let iter: Box<dyn Iterator<Item = T> + 'a> = Box::new(nodes.map(move |node| {
                    queue_clone
                        .borrow_mut()
                        .push_back(Bundle::new_subsequent(Next::Done(node.clone())));
                    node
                }));
                iter
            }
        }));
        let processed = neighbors(flattened);
        Self {
            inner: processed,
            queue: queue_clone,
        }
    }
}

impl<'a, T: Clone + 'a> Iterator for BundleMonad<'a, T> {
    type Item = Bundle<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        // See if queue has items
        if let Some(b) = self.queue.borrow_mut().pop_front() {
            return Some(b);
        }

        // Queue is empty, so generate some elements. We can't return
        // them though, since this also adds to the queue. Those elements
        // need to be returned first. If not, we will infinite-loop on
        // infinite-depth graphs.
        if let Some((_, neighbors_iter)) = self.inner.next() {
            self.queue
                .borrow_mut()
                .push_back(Bundle::new_subsequent(Next::Nodes(neighbors_iter)));
        }

        // Try reading from the queue again, since pulling
        // from self.inner might have added elements.
        if let Some(b) = self.queue.borrow_mut().pop_front() {
            return Some(b);
        }

        None
    }
}

struct Rec<'a, 'func, T, F>
where
    F: Fn(
        Box<dyn Iterator<Item = T> + 'a>,
    ) -> Box<dyn Iterator<Item = (T, Box<dyn Iterator<Item = T> + 'a>)> + 'a>,
{
    from: Option<IterBundle<'a, T>>,
    initial_neighbor_fn: &'func F,
    subsequent_neighbor_fn: &'func F,
}

impl<'a, 'func, T, F> Rec<'a, 'func, T, F>
where
    F: Fn(
        Box<dyn Iterator<Item = T> + 'a>,
    ) -> Box<dyn Iterator<Item = (T, Box<dyn Iterator<Item = T> + 'a>)> + 'a>,
{
    fn new(
        from: IterBundle<'a, T>,
        initial_neighbor_fn: &'func F,
        subsequent_neighbor_fn: &'func F,
    ) -> Self {
        Self {
            from: Some(from),
            initial_neighbor_fn,
            subsequent_neighbor_fn,
        }
    }
}

impl<'a, 'func, T: Clone + 'a, F> Iterator for Rec<'a, 'func, T, F>
where
    F: Fn(
        Box<dyn Iterator<Item = T> + 'a>,
    ) -> Box<dyn Iterator<Item = (T, Box<dyn Iterator<Item = T> + 'a>)> + 'a>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let peeked = self.from.as_mut().expect("a").peek();
            let bundle = match peeked {
                Some(b) => b,
                None => break,
            };

            if let Bundle {
                next_: Next::Nodes(..),
                ..
            } = bundle
            {
                let neighbors_fn = if bundle.is_initial {
                    self.initial_neighbor_fn
                } else {
                    self.subsequent_neighbor_fn
                };

                let taken_from = self.from.take().expect("'from' peek showed non-empty");
                let iter: Box<dyn Iterator<Item = Bundle<'a, T>>> =
                    Box::new(BundleMonad::bind(taken_from, neighbors_fn));
                self.from = Some(iter.peekable());
            } else {
                break;
            }
        }

        self.from
            .as_mut()
            .expect("c")
            .next()
            .map(|b| match b.next_ {
                Next::Done(x) => x,
                Next::Nodes(_) => panic!("AAA"),
            })
    }
}

// fn visit_self_and_neighbors<'query, DataToken: Clone + Debug + 'query>(
//     adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
//     query: &InterpretedQuery,
//     _component: &IRQueryComponent,
//     expanding_from_type: Arc<str>,
//     expanding_from: &IRVertex,
//     _expanding_to: &IRVertex,
//     edge_id: Eid,
//     edge_name: &Arc<str>,
//     edge_parameters: &Option<Arc<EdgeParameters>>,
//     source: IterBundle<'query, DataContext<DataToken>>,
// ) -> IterBundle<'query, DataContext<DataToken>> {
//     let iter = source.flat_map(move |b| -> Box<dyn Iterator<Item=Bundle<DataContext<DataToken>>> + 'query> {
//         match b.next_ {
//             Next::Done(x) => {
//                 Box::new(std::iter::once(Bundle {
//                     next_: Next::Done(x),
//                 }))
//             }
//             Next::Nodes(nodes) => {
//                 let mut adapter_ref = adapter.borrow_mut();
//                 let edge_iterator = adapter_ref.project_neighbors(
//                     nodes,
//                     expanding_from_type.clone(),
//                     edge_name.clone(),
//                     edge_parameters.clone(),
//                     query.clone(),
//                     expanding_from.vid,
//                     edge_id,
//                 );
//                 drop(adapter_ref);

//                 Box::new(edge_iterator.flat_map(move |(ctx, neighbors)| {
//                     let this_ctx = ctx.clone();
//                     let neighbors = Box::new(neighbors.map(move |node| {
//                         this_ctx.split_and_move_to_token(Some(node))
//                     }));
//                     [
//                         Bundle {
//                             next_: Next::Done(ctx),
//                         },
//                         Bundle {
//                             next_: Next::Nodes(neighbors),
//                         },
//                     ]
//                 }))
//             }
//         }
//     });
//     let iter: Box<dyn Iterator<Item=Bundle<DataContext<DataToken>>> + 'query> = Box::new(iter);
//     iter.peekable()
// }

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
fn coerce_and_resolve_neighbors<'query, DataToken>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    _component: &IRQueryComponent,
    expanding_from: &IRVertex,
    expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &Option<Arc<EdgeParameters>>,
    recursive: &Recursive,
    data_contexts: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<
    dyn Iterator<
            Item = (
                DataContext<DataToken>,
                Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
            ),
        > + 'query,
>
where
    DataToken: Clone + Debug + 'query,
{
    let edge_endpoint_type = expanding_to
        .coerced_from_type
        .as_ref()
        .unwrap_or(&expanding_to.type_name);

    let (traversal_from_type, expansion_base_iterator) =
        if let Some(coerce_to) = recursive.coerce_to.as_ref() {
            (
                coerce_to.clone(),
                perform_coercion(
                    &adapter,
                    query,
                    expanding_to,
                    edge_endpoint_type.clone(),
                    coerce_to.clone(),
                    data_contexts,
                ),
            )
        } else {
            (expanding_from.type_name.clone(), data_contexts)
        };

    let mut adapter_ref = adapter.borrow_mut();
    let edge_iterator = adapter_ref.project_neighbors(
        expansion_base_iterator,
        traversal_from_type,
        edge_name.clone(),
        edge_parameters.clone(),
        query.clone(),
        expanding_from.vid,
        edge_id,
    );
    drop(adapter_ref);

    Box::new(edge_iterator.map(|(context, neighbor_iterator)| {
        let neighbor_base = context.clone();
        let independent_neighbor_contexts: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> = Box::new(
            neighbor_iterator
                .map(move |neighbor| neighbor_base.split_and_move_to_token(Some(neighbor))),
        );
        (context, independent_neighbor_contexts)
    }))
}

// This is the current "expand an edge recursively" interface in execution.rs.
// Any replacement recursion logic will need to plug into this function,
// which execution.rs will call.
#[allow(clippy::too_many_arguments)]
#[allow(unused_variables)]
pub(super) fn expand_recursive_edge<'query, DataToken: Clone + Debug + 'query>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    component: &IRQueryComponent,
    expanding_from: &IRVertex,
    expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &Option<Arc<EdgeParameters>>,
    recursive: &Recursive,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    todo!()
}
