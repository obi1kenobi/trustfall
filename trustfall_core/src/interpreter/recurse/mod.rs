#![allow(dead_code)]
mod depth_layer;

use std::{cell::RefCell, collections::VecDeque, fmt::Debug, rc::Rc, sync::Arc};

use itertools::Itertools;

use crate::ir::{EdgeParameters, Eid, IRQueryComponent, IRVertex, Recursive};

use self::depth_layer::{BundleReader, DepthZeroReader, Layer, RcRecursionLayer, RecursionLayer};

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

    /// Recursive neighbor expansion args.
    edge_data: RecursiveEdgeData,

    /// Data structures that keep track of data at each recursion level.
    levels: Vec<Layer<'token, AdapterT::DataToken>>,

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
        let depth_zero = RcRecursionLayer::new(DepthZeroReader::new(data_contexts));

        Self {
            levels: vec![Layer::DepthZero(depth_zero)],
            adapter,
            edge_data,
            reorder_queue: VecDeque::new(),
            next_from: 0,
        }
    }

    fn increase_recursion_depth(&mut self) {
        let last_recursion_layer: Layer<_> = self
            .levels
            .last()
            .expect("at least one level exists")
            .clone();

        let (coercion_iter, bundle) = if self.levels.len() == 1 {
            self.edge_data
                .expand_initial_edge(self.adapter.as_ref(), Box::new(last_recursion_layer))
        } else {
            self.edge_data
                .expand_edge(self.adapter.as_ref(), Box::new(last_recursion_layer))
        };

        self.levels
            .push(Layer::Neighbors(RcRecursionLayer::new(BundleReader::new(
                coercion_iter,
                bundle,
            ))));

        // @recurse with depth N means the max allowed index in self.levels is N
        debug_assert!(self.levels.len() <= usize::from(self.edge_data.recursive_info.depth) + 1);
    }

    fn reorder_earlier_levels(&mut self, level: usize, ctx_pulls: usize) {
        // We must never have prepared more items than we have attempted to pull.
        assert!(ctx_pulls >= self.levels[level].total_prepared());

        while ctx_pulls > self.levels[level].total_prepared() {
            // The next level has pulled more elements than this level has prepared for it.
            // This usually happens when batching is used: the adapter impl requests multiple
            // elements from upstream before yielding its own results.
            //
            // In this case, we have to do some work to reorder the contexts into
            // their proper order using a queue:
            // - We grab the next non-prepared context and its pull index.
            // - We reorder earlier levels as needed based on that pull index.
            // - We push that non-prepared context onto the back of the queue.
            //
            // We keep looping until we've processed and correctly reordered all the contexts
            // that were not previously "prepared" due to the batching, so that the next level's
            // context at the specified `ctx_pulls` pull index goes next into the reorder queue.
            let (last_level_ctx, last_pulls) = self.levels[level]
                .pop_passed_unprepared()
                .expect("there was no unprepared token but the count said there should be");

            if level > 0 {
                self.reorder_earlier_levels(level - 1, last_pulls);
            }
            self.reorder_queue.push_back(last_level_ctx);
        }
    }

    /// The given context value is about to be produced from the Iterator::next() method.
    /// Ensure elements are produced in the correct order: if there are other contexts
    /// that need to be produced first, queue this context and return one from the queue instead.
    fn reorder_output(
        &mut self,
        level: usize,
        ctx: DataContext<AdapterT::DataToken>,
        ctx_pulls: usize,
    ) -> DataContext<AdapterT::DataToken> {
        if level > 0 {
            // We must never have prepared more items than we have attempted to pull.
            assert!(self.levels[level].total_pulls() >= self.levels[level - 1].total_prepared());

            // Include any other contexts that need to be output first.
            self.reorder_earlier_levels(level - 1, ctx_pulls);

            // If the reorder queue is non-empty, then the current context goes at the end of it.
            // Otherwise, we can just skip the queue and return the context directly.
            if let Some(new_ctx) = self.reorder_queue.pop_front() {
                self.reorder_queue.push_back(ctx);
                return new_ctx;
            }
        }

        ctx
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

        // @recurse with depth N means the max allowed index in self.levels is N
        debug_assert!(self.levels.len() <= usize::from(self.edge_data.recursive_info.depth) + 1);

        // Have we reached the maximum recursion depth?
        if self.levels.len() < usize::from(self.edge_data.recursive_info.depth) + 1 {
            // Add a new recursion level if necessary.
            if self.next_from == self.levels.len() {
                self.increase_recursion_depth();
            }
        }

        // Unless next_from is past the deepest level of the recursion,
        // we need to prepare an item from the next_from level.
        //
        // If self.next_from is 0, that's the "depth 0" level which is fine to prepare_with_pull().
        // Otherwise, we have 1 output prepared at level `self.next_from - 1`
        // because of the `self.next_from` invariant, so it's safe to prepare_with_pull().
        if let Some((ctx, ctx_pulls)) = self
            .levels
            .get_mut(self.next_from)
            .and_then(|level| level.prepare_with_pull())
        {
            // Increment so that the search behaves like depth-first search.
            self.next_from += 1;
            return Some(self.reorder_output(self.next_from - 1, ctx, ctx_pulls));
        }

        // If prepare_with_pull() at this level returned None, it's not safe to
        // try again since we don't have any more prepared inputs.
        //
        // Move up the stack until we find something with prepare_without_pull().
        while self.next_from > 1 {
            if let Some((ctx, ctx_pulls)) = self.levels[self.next_from - 1].prepare_without_pull() {
                return Some(self.reorder_output(self.next_from - 1, ctx, ctx_pulls));
            }
            self.next_from -= 1;
        }

        // If we've reached this point, then all the active neighbor iterators
        // at all recursion levels have run dry. Then it's time for a depth-0 element.
        debug_assert_eq!(self.next_from, 1);
        self.levels[0].prepare_with_pull().map(|(ctx, _)| ctx)
    }
}

// This is the current "expand an edge recursively" interface in execution.rs.
// Any replacement recursion logic will need to plug into this function,
// which execution.rs will call.
#[allow(clippy::too_many_arguments)]
#[allow(unused_variables)]
pub(in crate::interpreter) fn expand_recursive_edge<'query, DataToken: Clone + Debug + 'query>(
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
        expanding_from.clone(), // TODO: these shouldn't have to be cloned
        expanding_to.clone(),   // TODO: these shouldn't have to be cloned
        edge_id,
    );

    let stack = RecurseStack::new(adapter, iterator, edge_data);

    Box::new(stack)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crate::interpreter::DataContext;

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
            let schema_text = include_str!("../../resources/schemas/numbers.graphql");
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

    pub(super) mod simple_node_schema {
        pub(super) const SCHEMA_TEXT: &str = r#"
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

        pub(super) fn make_test_query(depth: usize) -> String {
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
    }

    struct BatchingIterator<'a, T, F>
    where
        F: Fn(&Option<String>) -> T,
    {
        input: Box<dyn Iterator<Item = DataContext<String>> + 'a>,
        resolver: F,
        buffer: VecDeque<(DataContext<String>, T)>,
        batch_size: usize,
    }

    impl<'a, T, F> BatchingIterator<'a, T, F>
    where
        F: Fn(&Option<String>) -> T,
    {
        fn new(
            input: Box<dyn Iterator<Item = DataContext<String>> + 'a>,
            resolver: F,
            batch_size: usize,
        ) -> Self {
            assert_ne!(batch_size, 0);
            Self {
                input,
                resolver,
                buffer: Default::default(),
                batch_size,
            }
        }
    }

    impl<'a, T, F> Iterator for BatchingIterator<'a, T, F>
    where
        F: Fn(&Option<String>) -> T,
    {
        type Item = (DataContext<String>, T);

        fn next(&mut self) -> Option<Self::Item> {
            if self.buffer.is_empty() {
                let mut remaining_batch_size = self.batch_size;

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

    mod adapter_variable_batching_does_not_change_result_order {
        use std::{cell::RefCell, collections::VecDeque, rc::Rc, sync::Arc};

        use crate::{
            frontend::parse,
            interpreter::{basic_adapter::*, execution, DataContext},
            schema::Schema,
        };

        struct VariableBatchingAdapter {
            base: usize,
            offset: usize,
            symbols: &'static [char],
            neighbor_calls: usize,
        }

        impl VariableBatchingAdapter {
            fn new(
                base_batch_size: usize,
                batch_size_offset: usize,
                symbols: &'static [char],
            ) -> Self {
                Self {
                    base: base_batch_size,
                    offset: batch_size_offset,
                    symbols,
                    neighbor_calls: 0,
                }
            }
        }

        impl BasicAdapter<'static> for VariableBatchingAdapter {
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
                                .map(move |suffix| value.to_owned() + suffix.to_string().as_str()),
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

        fn generate_and_validate_all_results(depth: usize, symbols: &'static [char]) {
            let schema =
                Schema::parse(super::simple_node_schema::SCHEMA_TEXT).expect("valid schema");
            let query = parse(&schema, super::simple_node_schema::make_test_query(depth))
                .expect("valid query");

            let mut all_results = vec![];
            for base_batch_size in 1..=symbols.len() {
                for offset in 1..=symbols.len() {
                    let adapter = Rc::new(RefCell::new(VariableBatchingAdapter::new(
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
            const SYMBOLS: &[char] = &['1', '2'];

            generate_and_validate_all_results(depth, SYMBOLS);
        }

        #[test]
        fn batching_does_not_change_result_order_at_depth_2_ply_4() {
            let depth = 2;
            const SYMBOLS: &[char] = &['1', '2', '3', '4'];

            generate_and_validate_all_results(depth, SYMBOLS);
        }

        #[test]
        fn batching_does_not_change_result_order_at_depth_2_ply_5() {
            let depth = 2;
            const SYMBOLS: &[char] = &['1', '2', '3', '4', '5'];

            generate_and_validate_all_results(depth, SYMBOLS);
        }

        #[test]
        fn batching_does_not_change_result_order_at_depth_4_ply_3() {
            let depth = 4;
            const SYMBOLS: &[char] = &['1', '2', '3'];

            generate_and_validate_all_results(depth, SYMBOLS);
        }
    }

    mod adapter_on_or_off_batching_does_not_change_result_order {
        use std::{cell::RefCell, collections::VecDeque, rc::Rc, sync::Arc};

        use crate::{
            frontend::parse,
            interpreter::{basic_adapter::*, execution},
            schema::Schema,
        };

        struct OnOrOffBatchingAdapter {
            property_batch_size: usize,
            neighbors_batch_sizes: VecDeque<usize>,
            symbols: &'static [char],
        }

        impl OnOrOffBatchingAdapter {
            fn new(
                property_batch_size: usize,
                neighbors_batch_sizes: VecDeque<usize>,
                symbols: &'static [char],
            ) -> Self {
                Self {
                    property_batch_size,
                    neighbors_batch_sizes,
                    symbols,
                }
            }
        }

        impl BasicAdapter<'static> for OnOrOffBatchingAdapter {
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

                Box::new(super::BatchingIterator::new(
                    contexts,
                    |value| value.clone().into(),
                    self.property_batch_size,
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

                let next_batch_size = self
                    .neighbors_batch_sizes
                    .pop_front()
                    .expect("sufficient batch sizes");

                Box::new(super::BatchingIterator::new(
                    contexts,
                    |value| {
                        let value = value
                            .as_ref()
                            .expect("no @optional in the test query")
                            .clone();
                        let neighbors: Box<dyn Iterator<Item = String> + 'static> = Box::new(
                            self.symbols
                                .iter()
                                .map(move |suffix| value.to_owned() + suffix.to_string().as_str()),
                        );
                        neighbors
                    },
                    next_batch_size,
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

        struct VariationsIterator<T: Copy> {
            next_state: usize,
            variations: usize,
            on_value: T,
            off_value: T,
        }

        impl<T: Copy> VariationsIterator<T> {
            fn new(variations: usize, on_value: T, off_value: T) -> Self {
                Self {
                    next_state: 0,
                    variations,
                    on_value,
                    off_value,
                }
            }
        }

        impl<T: Copy> Iterator for VariationsIterator<T> {
            type Item = VecDeque<T>;

            fn next(&mut self) -> Option<Self::Item> {
                if self.next_state == 1 << self.variations {
                    return None;
                }

                let mut output = VecDeque::with_capacity(self.variations);
                let mut state = self.next_state;
                for _ in 0..self.variations {
                    let next_value = if state & 1 != 0 {
                        self.on_value
                    } else {
                        self.off_value
                    };
                    output.push_back(next_value);
                    state >>= 1;
                }

                self.next_state += 1;
                Some(output)
            }
        }

        fn generate_and_validate_all_results(
            depth: usize,
            batch_size: usize,
            symbols: &'static [char],
        ) {
            assert!(batch_size > 1);
            assert!(batch_size <= symbols.len());

            let schema =
                Schema::parse(super::simple_node_schema::SCHEMA_TEXT).expect("valid schema");
            let query = parse(&schema, super::simple_node_schema::make_test_query(depth))
                .expect("valid query");

            let variations = VariationsIterator::new(depth, batch_size, 1);

            let mut all_results = vec![];
            for batch_schedule in variations {
                let adapter = Rc::new(RefCell::new(OnOrOffBatchingAdapter::new(
                    batch_size,
                    batch_schedule,
                    symbols,
                )));
                let results_iter =
                    execution::interpret_ir(adapter, query.clone(), Arc::new(Default::default()))
                        .expect("no execution errors");
                let results: Vec<_> = results_iter
                    .map(|x| x["value"].as_str().unwrap().to_string())
                    .collect();
                all_results.push(results);
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
        fn batching_does_not_change_result_order_at_depth_3_ply_6_batch_size_3() {
            let depth = 3;
            const SYMBOLS: &[char] = &['1', '2', '3', '4', '5', '6'];
            let batch_size = 3;

            generate_and_validate_all_results(depth, batch_size, SYMBOLS);
        }

        #[test]
        fn batching_does_not_change_result_order_at_depth_3_ply_9_batch_size_3() {
            let depth = 2;
            const SYMBOLS: &[char] = &['1', '2', '3', '4', '5', '6', '7', '8', '9'];
            let batch_size = 3;

            generate_and_validate_all_results(depth, batch_size, SYMBOLS);
        }

        #[test]
        fn batching_does_not_change_result_order_at_depth_4_ply_8_batch_size_4() {
            let depth = 4;
            const SYMBOLS: &[char] = &['1', '2', '3', '4', '5', '6', '7', '8'];
            let batch_size = 4;

            generate_and_validate_all_results(depth, batch_size, SYMBOLS);
        }
    }

    mod adapter_batching_where_not_all_edges_exist_does_not_change_result_order {
        use std::{cell::RefCell, rc::Rc, sync::Arc};

        use crate::{
            frontend::parse,
            interpreter::{basic_adapter::*, execution},
            schema::Schema,
        };

        struct NotAllEdgesExistAdapter {
            property_batch_size: usize,
            neighbors_batch_size: usize,
            symbols: &'static [char],
            symbols_with_edges: &'static [char],
        }

        impl NotAllEdgesExistAdapter {
            fn new(
                property_batch_size: usize,
                neighbors_batch_size: usize,
                symbols: &'static [char],
                symbols_with_edges: &'static [char],
            ) -> Self {
                Self {
                    property_batch_size,
                    neighbors_batch_size,
                    symbols,
                    symbols_with_edges,
                }
            }
        }

        impl BasicAdapter<'static> for NotAllEdgesExistAdapter {
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

                Box::new(super::BatchingIterator::new(
                    contexts,
                    |value| value.clone().into(),
                    self.property_batch_size,
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

                Box::new(super::BatchingIterator::new(
                    contexts,
                    |value| {
                        let value = value
                            .as_ref()
                            .expect("no @optional in the test query")
                            .clone();

                        let neighbors: Box<dyn Iterator<Item = String> + 'static> =
                            if value.ends_with(|c| self.symbols_with_edges.contains(&c)) {
                                Box::new(self.symbols.iter().map(move |suffix| {
                                    value.to_owned() + suffix.to_string().as_str()
                                }))
                            } else {
                                Box::new(std::iter::empty())
                            };
                        neighbors
                    },
                    self.neighbors_batch_size,
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

        fn generate_and_validate_result(
            depth: usize,
            batch_size: usize,
            symbols: &'static [char],
            symbols_with_edges: &'static [char],
        ) {
            assert!(batch_size > 1);
            assert!(batch_size <= symbols.len());

            let schema =
                Schema::parse(super::simple_node_schema::SCHEMA_TEXT).expect("valid schema");
            let query = parse(&schema, super::simple_node_schema::make_test_query(depth))
                .expect("valid query");

            let adapter = Rc::new(RefCell::new(NotAllEdgesExistAdapter::new(
                1,
                batch_size,
                symbols,
                symbols_with_edges,
            )));
            let results_iter =
                execution::interpret_ir(adapter, query, Arc::new(Default::default()))
                    .expect("no execution errors");
            let results: Vec<_> = results_iter
                .map(|x| x["value"].as_str().unwrap().to_string())
                .collect();

            dbg!(&results);

            // All the data points in each result are in lexicographic order.
            // This ensures that the recurse-produced ordering is correct.
            let mut last_value = &"0".to_string();
            for value in &results {
                assert!(last_value < value);
                last_value = value;
            }
        }

        #[test]
        fn not_all_edges_exist_depth_2_ply_5_edges_2_batch_2() {
            let depth = 2;
            let batch_size = 2;
            const SYMBOLS: &[char] = &['1', '2', '3', '4', '5'];
            const SYMBOLS_WITH_EDGES: &[char] = &['2', '4'];
            generate_and_validate_result(depth, batch_size, SYMBOLS, SYMBOLS_WITH_EDGES);
        }

        #[test]
        fn not_all_edges_exist_depth_4_ply_5_edges_2_batch_3() {
            let depth = 4;
            let batch_size = 3;
            const SYMBOLS: &[char] = &['1', '2', '3', '4', '5'];
            const SYMBOLS_WITH_EDGES: &[char] = &['2', '5'];
            generate_and_validate_result(depth, batch_size, SYMBOLS, SYMBOLS_WITH_EDGES);
        }
    }
}
