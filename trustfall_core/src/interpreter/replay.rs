use std::{
    cell::RefCell,
    collections::{btree_map, BTreeMap, VecDeque},
    convert::TryInto,
    fmt::Debug,
    marker::PhantomData,
    rc::Rc,
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::ir::{indexed::IndexedQuery, EdgeParameters, Eid, FieldValue, Vid};

use super::{
    execution::interpret_ir,
    trace::{FunctionCall, Opid, Trace, TraceOp, TraceOpContent, YieldValue},
    Adapter, DataContext, InterpretedQuery,
};

#[derive(Clone, Debug)]
struct TraceReaderAdapter<'trace, DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'trace,
    for<'de2> DataToken: Deserialize<'de2>,
{
    next_op: Rc<RefCell<btree_map::Iter<'trace, Opid, TraceOp<DataToken>>>>,
}

fn advance_ref_iter<T, Iter: Iterator<Item = T>>(iter: &RefCell<Iter>) -> Option<T> {
    // We do this through a separate function to ensure the mut borrow is dropped
    // as early as possible, to avoid overlapping mut borrows.
    iter.borrow_mut().next()
}

#[derive(Debug)]
struct TraceReaderStartingTokensIter<'trace, DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'trace,
    for<'de2> DataToken: Deserialize<'de2>,
{
    parent_opid: Opid,
    inner: Rc<RefCell<btree_map::Iter<'trace, Opid, TraceOp<DataToken>>>>,
}

#[allow(unused_variables)]
impl<'trace, DataToken> Iterator for TraceReaderStartingTokensIter<'trace, DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'trace,
    for<'de2> DataToken: Deserialize<'de2>,
{
    type Item = DataToken;

    fn next(&mut self) -> Option<Self::Item> {
        let (_, trace_op) = advance_ref_iter(self.inner.as_ref())
            .expect("Expected to have an item but found none.");
        assert_eq!(
            self.parent_opid,
            trace_op
                .parent_opid
                .expect("Expected an operation with a parent_opid.")
        );

        match &trace_op.content {
            TraceOpContent::OutputIteratorExhausted => None,
            TraceOpContent::YieldFrom(YieldValue::GetStartingTokens(token)) => Some(token.clone()),
            _ => unreachable!(),
        }
    }
}

struct TraceReaderProjectPropertiesIter<'trace, DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'trace,
    for<'de2> DataToken: Deserialize<'de2>,
{
    parent_opid: Opid,
    data_contexts: Box<dyn Iterator<Item = DataContext<DataToken>> + 'trace>,
    input_batch: VecDeque<DataContext<DataToken>>,
    inner: Rc<RefCell<btree_map::Iter<'trace, Opid, TraceOp<DataToken>>>>,
}

#[allow(unused_variables)]
impl<'trace, DataToken> Iterator for TraceReaderProjectPropertiesIter<'trace, DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'trace,
    for<'de2> DataToken: Deserialize<'de2>,
{
    type Item = (DataContext<DataToken>, FieldValue);

    fn next(&mut self) -> Option<Self::Item> {
        let next_op = loop {
            let (_, input_op) = advance_ref_iter(self.inner.as_ref())
                .expect("Expected to have an item but found none.");
            assert_eq!(
                self.parent_opid,
                input_op
                    .parent_opid
                    .expect("Expected an operation with a parent_opid."),
            );

            if let TraceOpContent::AdvanceInputIterator = &input_op.content {
                let input_data = self.data_contexts.next();

                let (_, input_op) = advance_ref_iter(self.inner.as_ref())
                    .expect("Expected to have an item but found none.");
                assert_eq!(
                    self.parent_opid,
                    input_op
                        .parent_opid
                        .expect("Expected an operation with a parent_opid."),
                );

                if let TraceOpContent::YieldInto(context) = &input_op.content {
                    let input_context = input_data.unwrap();
                    assert_eq!(context, &input_context);
                    self.input_batch.push_back(input_context);
                } else if let TraceOpContent::InputIteratorExhausted = &input_op.content {
                    assert_eq!(None, input_data);
                } else {
                    unreachable!();
                }
            } else {
                break input_op;
            }
        };

        match &next_op.content {
            TraceOpContent::YieldFrom(YieldValue::ProjectProperty(trace_context, value)) => {
                let input_context = self.input_batch.pop_front().unwrap();
                assert_eq!(trace_context, &input_context);
                Some((input_context, value.clone()))
            }
            TraceOpContent::OutputIteratorExhausted => {
                assert_eq!(None, self.input_batch.pop_front());
                None
            }
            _ => unreachable!(),
        }
    }
}

struct TraceReaderCanCoerceIter<'query, 'trace, DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'query,
    for<'de2> DataToken: Deserialize<'de2>,
    'trace: 'query,
{
    parent_opid: Opid,
    data_contexts: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
    input_batch: VecDeque<DataContext<DataToken>>,
    inner: Rc<RefCell<btree_map::Iter<'trace, Opid, TraceOp<DataToken>>>>,
}

#[allow(unused_variables)]
impl<'query, 'trace, DataToken> Iterator for TraceReaderCanCoerceIter<'query, 'trace, DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'query,
    for<'de2> DataToken: Deserialize<'de2>,
    'trace: 'query,
{
    type Item = (DataContext<DataToken>, bool);

    fn next(&mut self) -> Option<Self::Item> {
        let next_op = loop {
            let (_, input_op) = advance_ref_iter(self.inner.as_ref())
                .expect("Expected to have an item but found none.");
            assert_eq!(
                self.parent_opid,
                input_op
                    .parent_opid
                    .expect("Expected an operation with a parent_opid."),
            );

            if let TraceOpContent::AdvanceInputIterator = &input_op.content {
                let input_data = self.data_contexts.next();

                let (_, input_op) = advance_ref_iter(self.inner.as_ref())
                    .expect("Expected to have an item but found none.");
                assert_eq!(
                    self.parent_opid,
                    input_op
                        .parent_opid
                        .expect("Expected an operation with a parent_opid."),
                );

                if let TraceOpContent::YieldInto(context) = &input_op.content {
                    let input_context = input_data.unwrap();
                    assert_eq!(context, &input_context);

                    self.input_batch.push_back(input_context);
                } else if let TraceOpContent::InputIteratorExhausted = &input_op.content {
                    assert_eq!(None, input_data);
                } else {
                    unreachable!();
                }
            } else {
                break input_op;
            }
        };

        match &next_op.content {
            TraceOpContent::YieldFrom(YieldValue::CanCoerceToType(trace_context, can_coerce)) => {
                let input_context = self.input_batch.pop_front().unwrap();
                assert_eq!(trace_context, &input_context);
                Some((input_context, *can_coerce))
            }
            TraceOpContent::OutputIteratorExhausted => {
                assert_eq!(None, self.input_batch.pop_front());
                None
            }
            _ => unreachable!(),
        }
    }
}

struct TraceReaderProjectNeighborsIter<'query, 'trace, DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'query,
    for<'de2> DataToken: Deserialize<'de2>,
    'trace: 'query,
{
    parent_opid: Opid,
    data_contexts: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
    input_batch: VecDeque<DataContext<DataToken>>,
    inner: Rc<RefCell<btree_map::Iter<'trace, Opid, TraceOp<DataToken>>>>,
}

impl<'query, 'trace, DataToken> Iterator
    for TraceReaderProjectNeighborsIter<'query, 'trace, DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'query,
    for<'de2> DataToken: Deserialize<'de2>,
    'trace: 'query,
{
    type Item = (
        DataContext<DataToken>,
        Box<dyn Iterator<Item = DataToken> + 'query>,
    );

    fn next(&mut self) -> Option<Self::Item> {
        let next_op = loop {
            let (_, input_op) = advance_ref_iter(self.inner.as_ref())
                .expect("Expected to have an item but found none.");
            assert_eq!(
                self.parent_opid,
                input_op
                    .parent_opid
                    .expect("Expected an operation with a parent_opid."),
            );

            if let TraceOpContent::AdvanceInputIterator = &input_op.content {
                let input_data = self.data_contexts.next();

                let (_, input_op) = advance_ref_iter(self.inner.as_ref())
                    .expect("Expected to have an item but found none.");
                assert_eq!(
                    self.parent_opid,
                    input_op
                        .parent_opid
                        .expect("Expected an operation with a parent_opid."),
                );

                if let TraceOpContent::YieldInto(context) = &input_op.content {
                    let input_context = input_data.unwrap();
                    assert_eq!(context, &input_context);

                    self.input_batch.push_back(input_context);
                } else if let TraceOpContent::InputIteratorExhausted = &input_op.content {
                    assert_eq!(None, input_data);
                } else {
                    unreachable!();
                }
            } else {
                break input_op;
            }
        };

        match &next_op.content {
            TraceOpContent::YieldFrom(YieldValue::ProjectNeighborsOuter(trace_context)) => {
                let input_context = self.input_batch.pop_front().unwrap();
                assert_eq!(trace_context, &input_context);

                let neighbors = Box::new(TraceReaderNeighborIter {
                    parent_iterator_opid: next_op.opid,
                    next_index: 0,
                    inner: self.inner.clone(),
                    _phantom: PhantomData,
                });
                Some((input_context, neighbors))
            }
            TraceOpContent::OutputIteratorExhausted => {
                assert_eq!(None, self.input_batch.pop_front());
                None
            }
            _ => unreachable!(),
        }
    }
}

struct TraceReaderNeighborIter<'query, 'trace, DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'query,
    for<'de2> DataToken: Deserialize<'de2>,
    'trace: 'query,
{
    parent_iterator_opid: Opid,
    next_index: usize,
    inner: Rc<RefCell<btree_map::Iter<'trace, Opid, TraceOp<DataToken>>>>,
    _phantom: PhantomData<&'query ()>,
}

impl<'query, 'trace, DataToken> Iterator for TraceReaderNeighborIter<'query, 'trace, DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'query,
    for<'de2> DataToken: Deserialize<'de2>,
    'trace: 'query,
{
    type Item = DataToken;

    fn next(&mut self) -> Option<Self::Item> {
        let (_, trace_op) = advance_ref_iter(self.inner.as_ref())
            .expect("Expected to have an item but found none.");
        assert_eq!(
            self.parent_iterator_opid,
            trace_op
                .parent_opid
                .expect("Expected an operation with a parent_opid.")
        );

        match &trace_op.content {
            TraceOpContent::OutputIteratorExhausted => None,
            TraceOpContent::YieldFrom(YieldValue::ProjectNeighborsInner(index, token)) => {
                assert_eq!(self.next_index, *index);
                self.next_index += 1;
                Some(token.clone())
            }
            _ => unreachable!(),
        }
    }
}

#[allow(unused_variables)]
impl<'trace, DataToken> Adapter<'trace> for TraceReaderAdapter<'trace, DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'trace,
    for<'de2> DataToken: Deserialize<'de2>,
{
    type DataToken = DataToken;

    fn get_starting_tokens(
        &mut self,
        edge: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken> + 'trace> {
        let (root_opid, trace_op) = advance_ref_iter(self.next_op.as_ref())
            .expect("Expected a get_starting_tokens() call operation, but found none.");
        assert_eq!(None, trace_op.parent_opid);

        if let TraceOpContent::Call(FunctionCall::GetStartingTokens(vid)) = trace_op.content {
            assert_eq!(vid, vertex_hint);

            Box::new(TraceReaderStartingTokensIter {
                parent_opid: *root_opid,
                inner: self.next_op.clone(),
            })
        } else {
            unreachable!()
        }
    }

    fn project_property(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'trace>,
        current_type_name: Arc<str>,
        field_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, FieldValue)> + 'trace> {
        let (root_opid, trace_op) = advance_ref_iter(self.next_op.as_ref())
            .expect("Expected a project_property() call operation, but found none.");
        assert_eq!(None, trace_op.parent_opid);

        if let TraceOpContent::Call(FunctionCall::ProjectProperty(vid, type_name, property)) =
            &trace_op.content
        {
            assert_eq!(*vid, vertex_hint);
            assert_eq!(*type_name, current_type_name);
            assert_eq!(*property, field_name);

            Box::new(TraceReaderProjectPropertiesIter {
                parent_opid: *root_opid,
                data_contexts,
                input_batch: Default::default(),
                inner: self.next_op.clone(),
            })
        } else {
            unreachable!()
        }
    }

    #[allow(clippy::type_complexity)]
    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'trace>,
        current_type_name: Arc<str>,
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
        edge_hint: Eid,
    ) -> Box<
        dyn Iterator<
                Item = (
                    DataContext<Self::DataToken>,
                    Box<dyn Iterator<Item = Self::DataToken> + 'trace>,
                ),
            > + 'trace,
    > {
        let (root_opid, trace_op) = advance_ref_iter(self.next_op.as_ref())
            .expect("Expected a project_property() call operation, but found none.");
        assert_eq!(None, trace_op.parent_opid);

        if let TraceOpContent::Call(FunctionCall::ProjectNeighbors(vid, type_name, eid)) =
            &trace_op.content
        {
            assert_eq!(vid, &vertex_hint);
            assert_eq!(type_name, &current_type_name);
            assert_eq!(eid, &edge_hint);

            Box::new(TraceReaderProjectNeighborsIter {
                parent_opid: *root_opid,
                data_contexts,
                input_batch: Default::default(),
                inner: self.next_op.clone(),
            })
        } else {
            unreachable!()
        }
    }

    fn can_coerce_to_type(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'trace>,
        current_type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, bool)> + 'trace> {
        let (root_opid, trace_op) = advance_ref_iter(self.next_op.as_ref())
            .expect("Expected a can_coerce_to_type() call operation, but found none.");
        assert_eq!(None, trace_op.parent_opid);

        if let TraceOpContent::Call(FunctionCall::CanCoerceToType(vid, from_type, to_type)) =
            &trace_op.content
        {
            assert_eq!(*vid, vertex_hint);
            assert_eq!(*from_type, current_type_name);
            assert_eq!(*to_type, coerce_to_type_name);

            Box::new(TraceReaderCanCoerceIter {
                parent_opid: *root_opid,
                data_contexts,
                input_batch: Default::default(),
                inner: self.next_op.clone(),
            })
        } else {
            unreachable!()
        }
    }
}

#[allow(dead_code)]
pub fn assert_interpreted_results<'query, 'trace, DataToken>(
    trace: &Trace<DataToken>,
    expected_results: &[BTreeMap<Arc<str>, FieldValue>],
    complete: bool,
) where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'query,
    for<'de2> DataToken: Deserialize<'de2>,
    'trace: 'query,
{
    let next_op = Rc::new(RefCell::new(trace.ops.iter()));
    let trace_reader_adapter = Rc::new(RefCell::new(TraceReaderAdapter { next_op }));

    let query: Arc<IndexedQuery> = Arc::new(trace.ir_query.clone().try_into().unwrap());
    let arguments = Arc::new(
        trace
            .arguments
            .iter()
            .map(|(k, v)| (Arc::from(k.to_owned()), v.clone()))
            .collect(),
    );
    let mut trace_iter = interpret_ir(trace_reader_adapter, query, arguments).unwrap();
    let mut expected_iter = expected_results.iter();

    loop {
        let expected_row = expected_iter.next();
        let trace_row = trace_iter.next();

        if expected_row.is_none() {
            if complete {
                assert_eq!(None, trace_row);
            }
            return;
        } else {
            assert_eq!(expected_row, trace_row.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fmt::Debug,
        fs,
        path::{Path, PathBuf},
    };

    use serde::{Deserialize, Serialize};
    use trustfall_filetests_macros::parameterize;

    use crate::{
        filesystem_interpreter::FilesystemToken,
        interpreter::replay::assert_interpreted_results,
        numbers_interpreter::NumbersToken,
        util::{TestIRQuery, TestIRQueryResult, TestInterpreterOutputTrace},
    };

    fn check_trace<Token>(expected_ir: TestIRQuery, test_data: TestInterpreterOutputTrace<Token>)
    where
        Token: Debug + Clone + PartialEq + Eq + Serialize,
        for<'de> Token: Deserialize<'de>,
    {
        // Ensure that the trace file's IR hasn't drifted away from the IR file of the same name.
        assert_eq!(expected_ir.ir_query, test_data.trace.ir_query);
        assert_eq!(expected_ir.arguments, test_data.trace.arguments);

        assert_interpreted_results(&test_data.trace, &test_data.results, true);
    }

    fn check_filesystem_trace(expected_ir: TestIRQuery, input_data: &str) {
        if let Ok(test_data) =
            ron::from_str::<TestInterpreterOutputTrace<FilesystemToken>>(input_data)
        {
            assert_eq!(expected_ir.schema_name, "filesystem");
            assert_eq!(test_data.schema_name, "filesystem");
            check_trace(expected_ir, test_data);
        } else {
            unreachable!()
        }
    }

    fn check_numbers_trace(expected_ir: TestIRQuery, input_data: &str) {
        if let Ok(test_data) = ron::from_str::<TestInterpreterOutputTrace<NumbersToken>>(input_data)
        {
            assert_eq!(expected_ir.schema_name, "numbers");
            assert_eq!(test_data.schema_name, "numbers");
            check_trace(expected_ir, test_data);
        } else {
            unreachable!()
        }
    }

    #[parameterize("trustfall_core/src/resources/test_data/valid_queries")]
    fn parameterized_tester(base: &Path, stem: &str) {
        let mut input_path = PathBuf::from(base);
        input_path.push(format!("{}.trace.ron", stem));

        let input_data = fs::read_to_string(input_path).unwrap();

        let mut check_path = PathBuf::from(base);
        check_path.push(format!("{}.ir.ron", stem));
        let check_data = fs::read_to_string(check_path).unwrap();
        let expected_ir: TestIRQueryResult = ron::from_str(&check_data).unwrap();
        let expected_ir = expected_ir.unwrap();

        match expected_ir.schema_name.as_str() {
            "filesystem" => check_filesystem_trace(expected_ir, input_data.as_str()),
            "numbers" => check_numbers_trace(expected_ir, input_data.as_str()),
            _ => unreachable!("{}", expected_ir.schema_name),
        }
    }
}
