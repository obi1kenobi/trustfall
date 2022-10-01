#![allow(dead_code)]
use std::{cell::RefCell, collections::VecDeque, fmt::Debug, rc::Rc, sync::Arc};

use crate::ir::{EdgeParameters, Eid, IRQueryComponent, IRVertex, Recursive};

use super::{execution::perform_coercion, Adapter, DataContext, InterpretedQuery};

type NeighborsBundle<'token, Token> = Box<
    dyn Iterator<Item = (DataContext<Token>, Box<dyn Iterator<Item = Token> + 'token>)> + 'token,
>;

/// Arguments for the project_neighbors() calls.
pub(super) struct RecursiveEdgeData {
    edge_name: Arc<str>,
    parameters: Option<Arc<EdgeParameters>>,
    recursive_info: Recursive,
    query_hint: InterpretedQuery,
    expanding_from: IRVertex,
    expanding_to: IRVertex,
    edge_hint: Eid,
}

impl RecursiveEdgeData {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        recursive_info: Recursive,
        query_hint: InterpretedQuery,
        expanding_from: IRVertex,
        expanding_to: IRVertex,
        edge_hint: Eid,
    ) -> Self {
        Self {
            edge_name,
            parameters,
            recursive_info,
            query_hint,
            expanding_from,
            expanding_to,
            edge_hint,
        }
    }

    fn expand_initial_edge<'token, AdapterT: Adapter<'token> + 'token>(
        &self,
        adapter: &RefCell<AdapterT>,
        data_contexts: Box<dyn Iterator<Item = DataContext<AdapterT::DataToken>> + 'token>,
    ) -> NeighborsBundle<'token, AdapterT::DataToken> {
        adapter.borrow_mut().project_neighbors(
            data_contexts,
            self.expanding_from.type_name.clone(),
            self.edge_name.clone(),
            self.parameters.clone(),
            self.query_hint.clone(),
            self.expanding_from.vid,
            self.edge_hint,
        )
    }

    fn expand_edge<'token, AdapterT: Adapter<'token> + 'token>(
        &self,
        adapter: &RefCell<AdapterT>,
        data_contexts: Box<dyn Iterator<Item = DataContext<AdapterT::DataToken>> + 'token>,
    ) -> NeighborsBundle<'token, AdapterT::DataToken> {
        let edge_endpoint_type = self
            .expanding_to
            .coerced_from_type
            .as_ref()
            .unwrap_or(&self.expanding_to.type_name);

        let (traversal_from_type, expansion_base_iterator) =
            if let Some(coerce_to) = self.recursive_info.coerce_to.as_ref() {
                (
                    coerce_to.clone(),
                    perform_coercion(
                        adapter,
                        &self.query_hint,
                        &self.expanding_to,
                        edge_endpoint_type.clone(),
                        coerce_to.clone(),
                        data_contexts,
                    ),
                )
            } else {
                (self.expanding_from.type_name.clone(), data_contexts)
            };

        adapter.borrow_mut().project_neighbors(
            expansion_base_iterator,
            traversal_from_type,
            self.edge_name.clone(),
            self.parameters.clone(),
            self.query_hint.clone(),
            self.expanding_from.vid,
            self.edge_hint,
        )
    }
}

pub(super) struct RecurseStack<'token, AdapterT>
where
    AdapterT: Adapter<'token> + 'token,
{
    adapter: Rc<RefCell<AdapterT>>,
    starting_contexts: Option<Box<dyn Iterator<Item = DataContext<AdapterT::DataToken>> + 'token>>,

    /// Recursive neighbor expansion args.
    edge_data: RecursiveEdgeData,

    // /// The maximum depth of the recursion; None means unbounded i.e. "as long as there's data."
    // max_depth: Option<NonZeroUsize>,
    /// Data structures that keep track of data at each recursion level.
    levels: Vec<RcBundleReader<'token, AdapterT::DataToken>>,

    /// Queue to ensure elements are returned in correct order.
    reorder_queue: VecDeque<DataContext<AdapterT::DataToken>>,

    /// Largest index which is guaranteed to have data which
    /// we can peek() without advancing the parent level's iterator.
    next_from: usize,
}

impl<'token, AdapterT> RecurseStack<'token, AdapterT>
where
    AdapterT: Adapter<'token> + 'token,
{
    pub(super) fn new(
        adapter: Rc<RefCell<AdapterT>>,
        data_contexts: Box<dyn Iterator<Item = DataContext<AdapterT::DataToken>> + 'token>,
        edge_data: RecursiveEdgeData,
    ) -> Self {
        Self {
            starting_contexts: Some(data_contexts),
            levels: vec![],
            adapter,
            edge_data,
            reorder_queue: VecDeque::new(),
            next_from: 0,
        }
    }

    fn increase_recursion_depth(&mut self) {
        if self.levels.is_empty() {
            self.levels.push(RcBundleReader::new(
                self.edge_data.expand_initial_edge(
                    self.adapter.as_ref(),
                    self.starting_contexts
                        .take()
                        .expect("levels was empty but the starting contexts was None"),
                ),
            ))
        } else {
            let last_recursion_layer = RcBundleReader::clone(
                self.levels
                    .last()
                    .as_ref()
                    .expect("no recursion levels found"),
            );
            let new_recursion_layer = RcBundleReader::new(
                self.edge_data
                    .expand_edge(self.adapter.as_ref(), Box::new(last_recursion_layer)),
            );
            self.levels.push(new_recursion_layer);
        }

        debug_assert!(self.levels.len() <= usize::from(self.edge_data.recursive_info.depth));
    }

    /// The context value is about to be produced from the Iterator::next() method.
    /// Ensure elements are produced in the correct order: if there are other contexts
    /// that need to be produced first, queue this context and return one from the queue instead.
    fn reorder_output(
        &mut self,
        level: usize,
        ctx: DataContext<AdapterT::DataToken>,
    ) -> DataContext<AdapterT::DataToken> {
        let mut returned_ctx = ctx;

        if level > 0 {
            while self.levels[level].total_pulls() > self.levels[level - 1].total_prepared() {
                // N.B.: Do not reorder these lines!
                //       Lots of tricky interactions through Rc<RefCell<...>> here.
                //
                // This level has pulled more elements than the last level prepared for us.
                // This usually happens when batching is used: the adapter impl requests multiple
                // elements from upstream before yielding its own results.
                //
                // In this case, we have to do some work to reorder the tokens into
                // their proper order:
                // - We get an "unprepared" token from the previous level.
                // - We recursively reorder tokens on the previous level.
                // - We queue the token we were going to return.
                //   It's tempting to move this line to earlier, but it will mess up
                //   the recursive step!
                // - The token from the recursive step is our next candidate token to return.
                //
                // We keep looping until we've processed and correctly reordered all the tokens
                // that were not previously "prepared" due to the batching.
                let last_level_ctx = self.levels[level - 1]
                    .pop_passed_unprepared()
                    .expect("there was no unprepared token but the count said there should be");
                let next_ctx = self.reorder_output(level - 1, last_level_ctx);
                self.reorder_queue.push_back(returned_ctx);
                returned_ctx = next_ctx;
            }
        }

        returned_ctx
    }
}

impl<'token, AdapterT> Iterator for RecurseStack<'token, AdapterT>
where
    AdapterT: Adapter<'token> + 'token,
{
    type Item = DataContext<AdapterT::DataToken>;

    fn next(&mut self) -> Option<Self::Item> {
        let maybe_ctx = self.reorder_queue.pop_front();
        if maybe_ctx.is_some() {
            return maybe_ctx;
        }

        debug_assert!(self.levels.len() <= usize::from(self.edge_data.recursive_info.depth));

        // Have we reached the maximum recursion depth?
        if self.levels.len() < usize::from(self.edge_data.recursive_info.depth) {
            // Add a new recursion level if necessary.
            if self.next_from == self.levels.len() {
                self.increase_recursion_depth();
            }
        }

        // Unless next_from is at the bottom level of the recursion,
        // we need to prepare an item from the next_from level.
        //
        // We have 1 output prepared at level self.next_from - 1
        // because of the self.next_from invariant, so it's safe
        // to prepare(1).
        if let Some(ctx) = self
            .levels
            .get_mut(self.next_from)
            .and_then(|level| level.prepare(1))
        {
            // Increment so that the search behaves like depth-first search.
            self.next_from += 1;
            return Some(self.reorder_output(self.next_from - 1, ctx));
        }

        // If prepare(1) at this level returned None, it's not safe to
        // try again since we don't have inputs prepared anymore. Try to
        // prepare using prepare(0). Move up the stack until we find
        // something.
        while self.next_from > 0 {
            if let Some(ctx) = self.levels[self.next_from - 1].prepare(0) {
                return Some(self.reorder_output(self.next_from - 1, ctx));
            }
            self.next_from -= 1;
        }

        None
    }
}

pub(super) struct BundleReader<'token, Token>
where
    Token: Clone + Debug + 'token,
{
    inner: NeighborsBundle<'token, Token>,

    /// The source context and neighbors that we're in the middle of expanding.
    source: Option<DataContext<Token>>,
    buffer: Box<dyn Iterator<Item = Token> + 'token>,

    /// Number of times pulled buffers from the bundle
    total_pulls: usize,

    /// Number of outputs so far
    total_prepared: usize,

    /// Items already seen (and probably recorded as outputs), but not yet pulled from next().
    prepared: VecDeque<DataContext<Token>>,

    /// Items pulled from next() without being prepared first. This can happen with batching.
    passed_unprepared: VecDeque<DataContext<Token>>,
}

impl<'token, Token> Debug for BundleReader<'token, Token>
where
    Token: Debug + Clone + 'token,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BundleReader")
            .field("inner", &"<elided>")
            .field("source", &self.source)
            .field("buffer", &"<elided>")
            .field("total_pulls", &self.total_pulls)
            .field("total_prepared", &self.total_prepared)
            .field("prepared", &self.prepared)
            .field("passed_unprepared", &self.passed_unprepared)
            .finish()
    }
}

impl<'token, Token> BundleReader<'token, Token>
where
    Token: Clone + Debug + 'token,
{
    pub(super) fn new(bundle: NeighborsBundle<'token, Token>) -> Self {
        Self {
            inner: bundle,
            source: None,
            buffer: Box::new(std::iter::empty()),
            total_pulls: 0,
            total_prepared: 0,
            prepared: Default::default(),
            passed_unprepared: Default::default(),
        }
    }

    pub(super) fn pop_passed_unprepared(&mut self) -> Option<DataContext<Token>> {
        let maybe_token = self.passed_unprepared.pop_front();
        if maybe_token.is_some() {
            self.total_prepared += 1;
        }
        maybe_token
    }

    /// Prepare while not pulling more than specified from the bundle,
    /// i.e. from the parent level of the recursion.
    pub(super) fn prepare(&mut self, mut pull_limit: usize) -> Option<DataContext<Token>> {
        loop {
            let maybe_token = self.pop_passed_unprepared();
            if maybe_token.is_some() {
                return maybe_token;
            }

            if let Some(token) = self.buffer.next() {
                let neighbor_ctx = self
                    .source
                    .as_ref()
                    .expect("no source for existing buffer")
                    .split_and_move_to_token(Some(token));
                self.prepared.push_back(neighbor_ctx.clone());
                self.total_prepared += 1;
                return Some(neighbor_ctx);
            }

            if pull_limit == 0 {
                return None;
            }
            if let Some((new_source, new_buffer)) = self.inner.next() {
                self.total_pulls += 1;
                self.source = Some(new_source);
                self.buffer = new_buffer;
            } else {
                return None;
            }
            pull_limit -= 1;
        }
    }

    pub(super) fn total_prepared(&self) -> usize {
        self.total_prepared
    }

    pub(super) fn total_pulls(&self) -> usize {
        self.total_pulls
    }
}

impl<'token, Token> Iterator for BundleReader<'token, Token>
where
    Token: Clone + Debug + 'token,
{
    type Item = DataContext<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let maybe_token = self.prepared.pop_front();
            if maybe_token.is_some() {
                // We found a prepared element, return it.
                return maybe_token;
            }

            // If the adapter isn't using batching, then the next() call here will always be None
            // because any elements from the buffer were processed in a prior prepare().
            //
            // If the adapter is using batching, we might end up pulling from the buffer here,
            // so we'll also have to put those items back in the queue for prepare().
            // If we forgot to do that, those intermediate nodes in the recursion would not
            // be included in the recursive expansion of the edge.
            let maybe_token = self.buffer.next();
            if let Some(token) = maybe_token {
                let neighbor_ctx = self
                    .source
                    .as_ref()
                    .expect("no source for existing buffer")
                    .split_and_move_to_token(Some(token));
                self.passed_unprepared.push_back(neighbor_ctx.clone());
                return Some(neighbor_ctx);
            }

            if let Some((new_source, new_buffer)) = self.inner.next() {
                self.total_pulls += 1;
                self.source = Some(new_source);
                self.buffer = new_buffer;
            } else {
                // No more data.
                return None;
            }
        }
    }
}

/// Iterator with interior mutability. You can have two references to
/// it and call next() on either, just not at the same time.
#[derive(Debug, Clone)]
pub(super) struct RcBundleReader<'token, Token>
where
    Token: Clone + Debug + 'token,
{
    inner: Rc<RefCell<BundleReader<'token, Token>>>,
}

impl<'token, Token> RcBundleReader<'token, Token>
where
    Token: Clone + Debug + 'token,
{
    pub(super) fn new(bundle: NeighborsBundle<'token, Token>) -> Self {
        Self {
            inner: Rc::new(RefCell::new(BundleReader::new(bundle))),
        }
    }

    pub(super) fn pop_passed_unprepared(&mut self) -> Option<DataContext<Token>> {
        self.inner.as_ref().borrow_mut().pop_passed_unprepared()
    }

    pub(super) fn prepare(&mut self, pull_limit: usize) -> Option<DataContext<Token>> {
        self.inner.as_ref().borrow_mut().prepare(pull_limit)
    }

    pub(super) fn total_prepared(&self) -> usize {
        self.inner.as_ref().borrow().total_prepared
    }

    pub(super) fn total_pulls(&self) -> usize {
        self.inner.as_ref().borrow().total_pulls
    }
}

impl<'token, Token: Debug + Clone + 'token> Iterator for RcBundleReader<'token, Token> {
    type Item = DataContext<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.as_ref().borrow_mut().next()
    }
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
    let edge_data = RecursiveEdgeData::new(
        edge_name.clone(),
        edge_parameters.clone(),
        recursive.clone(),
        query.clone(),
        expanding_from.clone(), // these shouldn't have to be cloned
        expanding_to.clone(),   // these shouldn't have to be cloned
        edge_id,
    );

    let stack = RecurseStack::new(adapter, iterator, edge_data);

    Box::new(stack)
}
