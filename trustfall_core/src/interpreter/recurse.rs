#![allow(dead_code)]
use std::{cell::RefCell, collections::VecDeque, fmt::Debug, iter::Fuse, rc::Rc, sync::Arc};

use itertools::{Itertools, Tee};

use crate::ir::{EdgeParameters, Eid, IRQueryComponent, IRVertex, Recursive};

use super::{Adapter, DataContext, InterpretedQuery};

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

    #[allow(clippy::type_complexity)]
    fn expand_initial_edge<'token, AdapterT: Adapter<'token> + 'token>(
        &self,
        adapter: &RefCell<AdapterT>,
        data_contexts: Box<dyn Iterator<Item = DataContext<AdapterT::DataToken>> + 'token>,
    ) -> (
        Box<dyn Iterator<Item = (DataContext<AdapterT::DataToken>, bool)> + 'token>,
        NeighborsBundle<'token, AdapterT::DataToken>,
    ) {
        let (left, right) = data_contexts.tee();

        let coercion_iter = Box::new(left.map(|ctx| (ctx, true)));
        let bundle = adapter.borrow_mut().project_neighbors(
            Box::new(right),
            self.expanding_from.type_name.clone(),
            self.edge_name.clone(),
            self.parameters.clone(),
            self.query_hint.clone(),
            self.expanding_from.vid,
            self.edge_hint,
        );

        (coercion_iter, bundle)
    }

    #[allow(clippy::type_complexity)]
    fn expand_edge<'token, AdapterT: Adapter<'token> + 'token>(
        &self,
        adapter: &RefCell<AdapterT>,
        data_contexts: Box<dyn Iterator<Item = DataContext<AdapterT::DataToken>> + 'token>,
    ) -> (
        Box<dyn Iterator<Item = (DataContext<AdapterT::DataToken>, bool)> + 'token>,
        NeighborsBundle<'token, AdapterT::DataToken>,
    ) {
        let edge_endpoint_type = self
            .expanding_to
            .coerced_from_type
            .as_ref()
            .unwrap_or(&self.expanding_to.type_name);

        let (traversal_from_type, coercion_iter) =
            if let Some(coerce_to) = self.recursive_info.coerce_to.as_ref() {
                let mut adapter_ref = adapter.borrow_mut();
                let coercion_iter = adapter_ref.can_coerce_to_type(
                    data_contexts,
                    edge_endpoint_type.clone(),
                    coerce_to.clone(),
                    self.query_hint.clone(),
                    self.expanding_to.vid,
                );
                drop(adapter_ref);

                (coerce_to.clone(), coercion_iter)
            } else {
                let coercion_iter: Box<
                    dyn Iterator<Item = (DataContext<AdapterT::DataToken>, bool)> + 'token,
                > = Box::new(data_contexts.map(|ctx| (ctx, true)));

                (self.expanding_from.type_name.clone(), coercion_iter)
            };

        let (left, right) = coercion_iter.tee();

        let expansion_base_iterator: Box<
            dyn Iterator<Item = DataContext<AdapterT::DataToken>> + 'token,
        > = Box::new(right.flat_map(|(ctx, can_coerce)| can_coerce.then_some(ctx)));

        let bundle = adapter.borrow_mut().project_neighbors(
            expansion_base_iterator,
            traversal_from_type,
            self.edge_name.clone(),
            self.parameters.clone(),
            self.query_hint.clone(),
            self.expanding_from.vid,
            self.edge_hint,
        );

        (Box::new(left), bundle)
    }
}

pub(super) struct RecurseStack<'token, AdapterT>
where
    AdapterT: Adapter<'token> + 'token,
{
    adapter: Rc<RefCell<AdapterT>>,

    // Two views into the input iterator:
    // - one used for outputting depth 0 results into the recurse results,
    // - and one used to drive the depth 1 level of the recursion
    depth_zero_contexts: Tee<Box<dyn Iterator<Item = DataContext<AdapterT::DataToken>> + 'token>>,
    starting_contexts: Option<Box<dyn Iterator<Item = DataContext<AdapterT::DataToken>> + 'token>>,

    /// Recursive neighbor expansion args.
    edge_data: RecursiveEdgeData,

    /// Data structures that keep track of data at each recursion level.
    levels: Vec<RcBundleReader<'token, AdapterT::DataToken>>,

    /// Queue to ensure elements are returned in correct order.
    reorder_queue: VecDeque<DataContext<AdapterT::DataToken>>,

    /// Largest index which is guaranteed to have data which
    /// we can peek() without advancing the parent level's iterator.
    next_from: usize,

    depth_zero_advanced: bool,
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
        let (first, second) = data_contexts.tee();
        Self {
            depth_zero_contexts: first,
            starting_contexts: Some(Box::new(second)),
            levels: vec![],
            adapter,
            edge_data,
            reorder_queue: VecDeque::new(),
            next_from: 0,
            depth_zero_advanced: false,
        }
    }

    fn increase_recursion_depth(&mut self) {
        if self.levels.is_empty() {
            let (coercion_iter, bundle) = self.edge_data.expand_initial_edge(
                self.adapter.as_ref(),
                self.starting_contexts
                    .take()
                    .expect("levels was empty but the starting contexts was None"),
            );

            self.levels.push(RcBundleReader::new(coercion_iter, bundle))
        } else {
            let last_recursion_layer = RcBundleReader::clone(
                self.levels
                    .last()
                    .as_ref()
                    .expect("no recursion levels found"),
            );

            let (coercion_iter, bundle) = self
                .edge_data
                .expand_edge(self.adapter.as_ref(), Box::new(last_recursion_layer));
            let new_recursion_layer = RcBundleReader::new(coercion_iter, bundle);
            self.levels.push(new_recursion_layer);
        }

        debug_assert!(self.levels.len() <= usize::from(self.edge_data.recursive_info.depth));
    }

    /// The given context value is about to be produced from the Iterator::next() method.
    /// Ensure elements are produced in the correct order: if there are other contexts
    /// that need to be produced first, queue this context and return one from the queue instead.
    fn reorder_output(
        &mut self,
        level: usize,
        ctx: DataContext<AdapterT::DataToken>,
    ) -> DataContext<AdapterT::DataToken> {
        let mut returned_ctx = ctx;

        if level > 0 {
            // We must never have prepared more items than we have attempted to pull.
            assert!(self.levels[level].total_pulls() >= self.levels[level - 1].total_prepared());

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
        if !self.depth_zero_advanced {
            self.depth_zero_advanced = true;
            return self.depth_zero_contexts.next();
        }

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

        // Unless next_from is past the deepest level of the recursion,
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
        // try again since we don't have any more prepared inputs.
        // Try to prepare using prepare(0). Move up the stack until we find something.
        while self.next_from > 0 {
            if let Some(ctx) = self.levels[self.next_from - 1].prepare(0) {
                return Some(self.reorder_output(self.next_from - 1, ctx));
            }
            self.next_from -= 1;
        }

        // If we've reached this point, then all the active neighbor iterators
        // at all recursion levels have run dry. Then it's time for a depth-0 element.
        debug_assert_eq!(self.next_from, 0);
        self.depth_zero_contexts.next()
    }
}

pub(super) struct BundleReader<'token, Token>
where
    Token: Clone + Debug + 'token,
{
    coercion_iter: Fuse<Box<dyn Iterator<Item = (DataContext<Token>, bool)> + 'token>>,
    inner: Fuse<NeighborsBundle<'token, Token>>,

    /// The source context and neighbors that we're in the middle of expanding.
    source: Option<DataContext<Token>>,
    buffer: Fuse<Box<dyn Iterator<Item = Token> + 'token>>,

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
            .field("coercion_iter", &"<elided>")
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
    pub(super) fn new(
        coercion_iter: Box<dyn Iterator<Item = (DataContext<Token>, bool)> + 'token>,
        bundle: NeighborsBundle<'token, Token>,
    ) -> Self {
        let buffer: Box<dyn Iterator<Item = _> + 'token> = Box::new(std::iter::empty());
        Self {
            coercion_iter: coercion_iter.fuse(),
            inner: bundle.fuse(),
            source: None,
            buffer: buffer.fuse(),
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
            if let Some((_ctx, can_coerce)) = self.coercion_iter.next() {
                self.total_pulls += 1;
                if can_coerce {
                    // This element passes through the coercion,
                    // and will also be returned by self.inner.next().
                    let (new_source, new_buffer) = self.inner.next().expect(
                        "coercion returned coercible element but inner.next() returned None",
                    );
                    // debug_assert_eq!(ctx, new_source);
                    self.source = Some(new_source);
                    self.buffer = new_buffer.fuse();
                }
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

            loop {
                if let Some((_ctx, can_coerce)) = self.coercion_iter.next() {
                    self.total_pulls += 1;
                    if can_coerce {
                        // This element passes through the coercion,
                        // and will also be returned by self.inner.next().
                        let (new_source, new_buffer) = self.inner.next().expect(
                            "coercion returned coercible element but inner.next() returned None",
                        );
                        // debug_assert_eq!(ctx, new_source);
                        self.source = Some(new_source);
                        self.buffer = new_buffer.fuse();
                        break;
                    }
                } else {
                    // No more data.
                    return None;
                }
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
    pub(super) fn new(
        coercion_iter: Box<dyn Iterator<Item = (DataContext<Token>, bool)> + 'token>,
        bundle: NeighborsBundle<'token, Token>,
    ) -> Self {
        Self {
            inner: Rc::new(RefCell::new(BundleReader::new(coercion_iter, bundle))),
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

#[cfg(test)]
mod tests {
    mod first_result_expands_no_edges {
        use std::{cell::RefCell, rc::Rc, sync::Arc};

        use crate::{
            frontend::parse,
            interpreter::{basic_adapter::*, execution},
            ir::EdgeParameters,
            schema::Schema,
        };

        struct TestAdapter;

        impl TestAdapter {
            const TEST_QUERY: &'static str = r#"
                {
                    Number(max: 10) {
                        successor @recurse(depth: 3) {
                            value @output
                        }
                    }
                }
            "#;
        }

        impl BasicAdapter<'static> for TestAdapter {
            type Vertex = i64;

            fn resolve_starting_vertices(
                &mut self,
                edge_name: &str,
                parameters: Option<&crate::ir::EdgeParameters>,
            ) -> VertexIterator<'static, Self::Vertex> {
                // This adapter is only meant for one very specific query.
                assert_eq!(edge_name, "Number");
                assert_eq!(
                    parameters,
                    Some(&EdgeParameters(btreemap! {
                        Arc::from("min") => 0.into(),
                        Arc::from("max") => 10.into(),
                    }))
                );

                let mut invocations = 0usize;
                Box::new((0..10).map(move |num| {
                    invocations += 1;
                    assert!(
                        invocations <= 1,
                        "The iterator produced by resolve_starting_vertices() was advanced \
                        more than once. In this test case, this almost certainly means a buggy
                        (insufficiently-lazy) @recurse implementation."
                    );

                    num
                }))
            }

            fn resolve_property(
                &mut self,
                contexts: ContextIterator<'static, Self::Vertex>,
                type_name: &str,
                property_name: &str,
            ) -> ContextOutcomeIterator<'static, Self::Vertex, crate::ir::FieldValue> {
                // This adapter is only meant for one very specific query.
                assert_eq!(type_name, "Number");
                assert_eq!(property_name, "value");

                let mut invocations = 0usize;
                Box::new(contexts.map(move |ctx| {
                    invocations += 1;
                    assert!(
                        invocations <= 1,
                        "The iterator produced by resolve_starting_vertices() was advanced \
                        more than once. In this test case, this almost certainly means a buggy
                        (insufficiently-lazy) @recurse implementation."
                    );

                    let value = ctx.current_token.into();
                    (ctx, value)
                }))
            }

            fn resolve_neighbors(
                &mut self,
                _contexts: ContextIterator<'static, Self::Vertex>,
                type_name: &str,
                edge_name: &str,
                parameters: Option<&crate::ir::EdgeParameters>,
            ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>>
            {
                panic!(
                    "resolve_neighbors() was not expected to be called, but was called anyway. \
                    In this test case, this almost certainly means a buggy (insufficiently-lazy) \
                    @recurse implementation. \
                    Call arguments: {type_name} {edge_name} {parameters:#?}"
                )
            }

            fn resolve_coercion(
                &mut self,
                _contexts: ContextIterator<'static, Self::Vertex>,
                type_name: &str,
                coerce_to_type: &str,
            ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
                panic!(
                    "resolve_coercion() was not expected to be called, but was called with \
                    arguments: {type_name} {coerce_to_type}"
                )
            }
        }

        #[test]
        fn first_recurse_result_resolves_no_edges() {
            let adapter = Rc::new(RefCell::new(TestAdapter));
            let schema_text = include_str!("../resources/schemas/numbers.graphql");
            let schema = Schema::parse(schema_text).expect("valid schema");

            let indexed_query = parse(&schema, TestAdapter::TEST_QUERY).expect("valid query");
            let mut results_iter =
                execution::interpret_ir(adapter, indexed_query, Arc::new(Default::default()))
                    .expect("no execution errors");

            let first_result = results_iter.next().expect("there is at least one result");
            assert_eq!(
                btreemap! {
                    Arc::from("value") => 0.into()
                },
                first_result
            );
        }
    }

    mod adapter_batching_does_not_change_result_order {
        use std::{cell::RefCell, collections::VecDeque, rc::Rc, sync::Arc};

        use crate::{
            frontend::parse,
            interpreter::{basic_adapter::*, execution, DataContext},
            schema::Schema,
        };

        struct TestAdapter {
            base: usize,
            offset: usize,
            symbols: &'static [&'static str],
            neighbor_calls: usize,
        }

        impl TestAdapter {
            const SCHEMA_TEXT: &'static str = r#"
                schema {
                    query: RootSchemaQuery
                }
                directive @filter(op: String!, value: [String!]) on FIELD | INLINE_FRAGMENT
                directive @tag(name: String) on FIELD
                directive @output(name: String) on FIELD
                directive @optional on FIELD
                directive @recurse(depth: Int!) on FIELD
                directive @fold on FIELD
                directive @transform(op: String!) on FIELD

                type RootSchemaQuery {
                    Node: [Node!]!
                }

                type Node {
                    value: String!

                    next_layer: [Node!]!
                }
            "#;

            fn make_test_query(depth: usize) -> String {
                // The doubled `{{` and `}}` are because of format!()'s need
                // for escaping `{` and `}`.
                format!(
                    r#"
                    query {{
                        Node {{
                            next_layer @recurse(depth: {depth}) {{
                                value @output
                            }}
                        }}
                    }}
                "#
                )
            }

            fn new(
                base_batch_size: usize,
                batch_size_offset: usize,
                symbols: &'static [&'static str],
            ) -> Self {
                Self {
                    base: base_batch_size,
                    offset: batch_size_offset,
                    symbols,
                    neighbor_calls: 0,
                }
            }
        }

        impl BasicAdapter<'static> for TestAdapter {
            type Vertex = String;

            fn resolve_starting_vertices(
                &mut self,
                edge_name: &str,
                parameters: Option<&crate::ir::EdgeParameters>,
            ) -> VertexIterator<'static, Self::Vertex> {
                assert_eq!(edge_name, "Node");
                assert!(parameters.is_none());

                Box::new(self.symbols.iter().map(|x| x.to_string()))
            }

            fn resolve_property(
                &mut self,
                contexts: ContextIterator<'static, Self::Vertex>,
                type_name: &str,
                property_name: &str,
            ) -> ContextOutcomeIterator<'static, Self::Vertex, crate::ir::FieldValue> {
                assert_eq!(type_name, "Node");
                assert_eq!(property_name, "value");

                let property_batch_base = (self.base % self.symbols.len()) + 1;

                Box::new(VariableBatchingIterator::new(
                    contexts,
                    |value| value.clone().into(),
                    property_batch_base,
                    self.symbols.len(),
                ))
            }

            fn resolve_neighbors(
                &mut self,
                contexts: ContextIterator<'static, Self::Vertex>,
                type_name: &str,
                edge_name: &str,
                parameters: Option<&crate::ir::EdgeParameters>,
            ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>>
            {
                assert_eq!(type_name, "Node");
                assert_eq!(edge_name, "next_layer");
                assert!(parameters.is_none());

                self.neighbor_calls += 1;

                // Get different batch size start points for each neighbor resolution.
                let neighbors_batch_base =
                    ((self.base + (self.offset * self.neighbor_calls)) % self.symbols.len()) + 1;

                Box::new(VariableBatchingIterator::new(
                    contexts,
                    |value| {
                        let value = value
                            .as_ref()
                            .expect("no @optional in the test query")
                            .clone();
                        let neighbors: Box<dyn Iterator<Item = String> + 'static> = Box::new(
                            self.symbols
                                .iter()
                                .map(move |suffix| value.to_owned() + suffix),
                        );
                        neighbors
                    },
                    neighbors_batch_base,
                    self.symbols.len(),
                ))
            }

            fn resolve_coercion(
                &mut self,
                _contexts: ContextIterator<'static, Self::Vertex>,
                type_name: &str,
                coerce_to_type: &str,
            ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
                panic!(
                    "resolve_coercion() was not expected to be called, but was called with \
                    arguments: {type_name} {coerce_to_type}"
                )
            }
        }

        struct VariableBatchingIterator<'a, T, F>
        where
            F: Fn(&Option<String>) -> T,
        {
            input: Box<dyn Iterator<Item = DataContext<String>> + 'a>,
            resolver: F,
            buffer: VecDeque<(DataContext<String>, T)>,
            next_batch_size: usize,
            max_batch_size: usize,
        }

        impl<'a, T, F> VariableBatchingIterator<'a, T, F>
        where
            F: Fn(&Option<String>) -> T,
        {
            fn new(
                input: Box<dyn Iterator<Item = DataContext<String>> + 'a>,
                resolver: F,
                starting_batch_size: usize,
                max_batch_size: usize,
            ) -> Self {
                assert_ne!(starting_batch_size, 0);
                assert!(max_batch_size >= starting_batch_size);
                Self {
                    input,
                    resolver,
                    buffer: Default::default(),
                    next_batch_size: starting_batch_size,
                    max_batch_size,
                }
            }
        }

        impl<'a, T, F> Iterator for VariableBatchingIterator<'a, T, F>
        where
            F: Fn(&Option<String>) -> T,
        {
            type Item = (DataContext<String>, T);

            fn next(&mut self) -> Option<Self::Item> {
                if self.buffer.is_empty() {
                    let mut remaining_batch_size = self.next_batch_size;
                    self.next_batch_size = if self.next_batch_size == self.max_batch_size {
                        1
                    } else {
                        self.next_batch_size + 1
                    };

                    for item in &mut self.input {
                        assert!(remaining_batch_size > 0);

                        let value = (self.resolver)(&item.current_token);
                        self.buffer.push_back((item, value));

                        remaining_batch_size -= 1;
                        if remaining_batch_size == 0 {
                            break;
                        }
                    }
                }

                self.buffer.pop_front()
            }
        }

        fn generate_and_validate_all_results(depth: usize, symbols: &'static [&'static str]) {
            let schema = Schema::parse(TestAdapter::SCHEMA_TEXT).expect("valid schema");
            let query = parse(&schema, TestAdapter::make_test_query(depth)).expect("valid query");

            let mut all_results = vec![];
            for base_batch_size in 1..=symbols.len() {
                for offset in 1..=symbols.len() {
                    let adapter = Rc::new(RefCell::new(TestAdapter::new(
                        base_batch_size,
                        offset,
                        symbols,
                    )));
                    let results_iter = execution::interpret_ir(
                        adapter,
                        query.clone(),
                        Arc::new(Default::default()),
                    )
                    .expect("no execution errors");
                    let results: Vec<_> = results_iter
                        .map(|x| x["value"].as_str().unwrap().to_string())
                        .collect();
                    all_results.push(results);
                }
            }

            // We got some results.
            assert!(!all_results.is_empty());

            // All results are equal to each other.
            // This ensures that varying the batch size and sequence
            // does not change the order in which results are produced.
            let first_result = all_results.first().unwrap();
            for result in &all_results {
                assert_eq!(first_result, result);
            }

            // All the data points in each result are in lexicographic order.
            // This ensures that the recurse-produced ordering is correct.
            let mut last_value = &"0".to_string();
            for value in first_result {
                assert!(last_value < value);
                last_value = value;
            }
        }

        #[test]
        fn batching_does_not_change_result_order_at_depth_2_ply_2() {
            let depth = 2;
            const SYMBOLS: &[&str] = &["1", "2"];

            generate_and_validate_all_results(depth, SYMBOLS);
        }

        #[test]
        fn batching_does_not_change_result_order_at_depth_2_ply_5() {
            let depth = 2;
            const SYMBOLS: &[&str] = &["1", "2", "3", "4", "5"];

            generate_and_validate_all_results(depth, SYMBOLS);
        }

        #[test]
        fn batching_does_not_change_result_order_at_depth_4_ply_3() {
            let depth = 4;
            const SYMBOLS: &[&str] = &["1", "2", "3"];

            generate_and_validate_all_results(depth, SYMBOLS);
        }
    }
}
