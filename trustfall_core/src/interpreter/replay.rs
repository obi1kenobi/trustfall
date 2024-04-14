use std::{
    cell::RefCell,
    collections::{btree_map, BTreeMap, VecDeque},
    fmt::Debug,
    marker::PhantomData,
    rc::Rc,
    sync::Arc,
};

use serde::{de::DeserializeOwned, Serialize};

use crate::{
    interpreter::VertexInfo,
    ir::{EdgeParameters, FieldValue, IndexedQuery},
};

use super::{
    execution::interpret_ir,
    trace::{FunctionCall, Opid, Trace, TraceOp, TraceOpContent, YieldValue},
    Adapter, AsVertex, ContextIterator, ContextOutcomeIterator, DataContext, ResolveEdgeInfo,
    ResolveInfo, VertexIterator,
};

#[derive(Clone, Debug)]
struct TraceReaderAdapter<'trace, Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'trace,
{
    next_op: Rc<RefCell<btree_map::Iter<'trace, Opid, TraceOp<Vertex>>>>,
}

fn advance_ref_iter<T, Iter: Iterator<Item = T>>(iter: &RefCell<Iter>) -> Option<T> {
    // We do this through a separate function to ensure the mut borrow is dropped
    // as early as possible, to avoid overlapping mut borrows.
    iter.borrow_mut().next()
}

#[derive(Debug)]
struct TraceReaderStartingVerticesIter<'trace, Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'trace,
{
    exhausted: bool,
    parent_opid: Opid,
    inner: Rc<RefCell<btree_map::Iter<'trace, Opid, TraceOp<Vertex>>>>,
}

#[allow(unused_variables)]
impl<'trace, Vertex> Iterator for TraceReaderStartingVerticesIter<'trace, Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'trace,
{
    type Item = Vertex;

    fn next(&mut self) -> Option<Self::Item> {
        assert!(!self.exhausted);

        let (_, trace_op) = advance_ref_iter(self.inner.as_ref())
            .expect("Expected to have an item but found none.");
        assert_eq!(
            self.parent_opid,
            trace_op.parent_opid.expect("Expected an operation with a parent_opid."),
            "Expected parent_opid {:?} did not match operation {:#?}",
            self.parent_opid,
            trace_op,
        );

        match &trace_op.content {
            TraceOpContent::OutputIteratorExhausted => {
                self.exhausted = true;
                None
            }
            TraceOpContent::YieldFrom(YieldValue::ResolveStartingVertices(vertex)) => {
                Some(vertex.clone())
            }
            _ => unreachable!(),
        }
    }
}

struct TraceReaderResolvePropertiesIter<'trace, V, Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'trace,
{
    exhausted: bool,
    parent_opid: Opid,
    contexts: ContextIterator<'trace, V>,
    input_batch: VecDeque<DataContext<V>>,
    inner: Rc<RefCell<btree_map::Iter<'trace, Opid, TraceOp<Vertex>>>>,
}

#[allow(unused_variables)]
impl<'trace, V, Vertex> Iterator for TraceReaderResolvePropertiesIter<'trace, V, Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'trace,
    V: AsVertex<Vertex>,
{
    type Item = (DataContext<V>, FieldValue);

    fn next(&mut self) -> Option<Self::Item> {
        assert!(!self.exhausted);
        let next_op = loop {
            let (_, input_op) = advance_ref_iter(self.inner.as_ref())
                .expect("Expected to have an item but found none.");
            assert_eq!(
                self.parent_opid,
                input_op.parent_opid.expect("Expected an operation with a parent_opid."),
                "Expected parent_opid {:?} did not match operation {:#?}",
                self.parent_opid,
                input_op,
            );

            if let TraceOpContent::AdvanceInputIterator = &input_op.content {
                let input_data = self.contexts.next();

                let (_, input_op) = advance_ref_iter(self.inner.as_ref())
                    .expect("Expected to have an item but found none.");
                assert_eq!(
                    self.parent_opid,
                    input_op.parent_opid.expect("Expected an operation with a parent_opid."),
                    "Expected parent_opid {:?} did not match operation {:#?}",
                    self.parent_opid,
                    input_op,
                );

                if let TraceOpContent::YieldInto(context) = &input_op.content {
                    let input_context = input_data.unwrap();
                    assert_eq!(
                        context,
                        &input_context.clone().flat_map(&mut |v| v.into_vertex()),
                        "at {input_op:?}"
                    );
                    self.input_batch.push_back(input_context);
                } else if let TraceOpContent::InputIteratorExhausted = &input_op.content {
                    assert!(input_data.is_none(), "at {input_op:?}");
                } else {
                    unreachable!();
                }
            } else {
                break input_op;
            }
        };

        match &next_op.content {
            TraceOpContent::YieldFrom(YieldValue::ResolveProperty(trace_context, value)) => {
                let input_context = self.input_batch.pop_front().unwrap();
                assert_eq!(
                    trace_context,
                    &input_context.clone().flat_map(&mut |v| v.into_vertex()),
                    "at {next_op:?}"
                );
                Some((input_context, value.clone()))
            }
            TraceOpContent::OutputIteratorExhausted => {
                assert!(self.input_batch.pop_front().is_none(), "at {next_op:?}");
                self.exhausted = true;
                None
            }
            _ => unreachable!(),
        }
    }
}

struct TraceReaderResolveCoercionIter<'query, 'trace, V, Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'query,
    V: AsVertex<Vertex>,
    'trace: 'query,
{
    exhausted: bool,
    parent_opid: Opid,
    contexts: ContextIterator<'query, V>,
    input_batch: VecDeque<DataContext<V>>,
    inner: Rc<RefCell<btree_map::Iter<'trace, Opid, TraceOp<Vertex>>>>,
}

#[allow(unused_variables)]
impl<'query, 'trace, V, Vertex> Iterator
    for TraceReaderResolveCoercionIter<'query, 'trace, V, Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'query,
    V: AsVertex<Vertex>,
    'trace: 'query,
{
    type Item = (DataContext<V>, bool);

    fn next(&mut self) -> Option<Self::Item> {
        assert!(!self.exhausted);
        let next_op = loop {
            let (_, input_op) = advance_ref_iter(self.inner.as_ref())
                .expect("Expected to have an item but found none.");
            assert_eq!(
                self.parent_opid,
                input_op.parent_opid.expect("Expected an operation with a parent_opid."),
                "Expected parent_opid {:?} did not match operation {:#?}",
                self.parent_opid,
                input_op,
            );

            if let TraceOpContent::AdvanceInputIterator = &input_op.content {
                let input_data = self.contexts.next();

                let (_, input_op) = advance_ref_iter(self.inner.as_ref())
                    .expect("Expected to have an item but found none.");
                assert_eq!(
                    self.parent_opid,
                    input_op.parent_opid.expect("Expected an operation with a parent_opid."),
                    "Expected parent_opid {:?} did not match operation {:#?}",
                    self.parent_opid,
                    input_op,
                );

                if let TraceOpContent::YieldInto(context) = &input_op.content {
                    let input_context = input_data.unwrap();
                    assert_eq!(
                        context,
                        &input_context.clone().flat_map(&mut |v| v.into_vertex()),
                        "at {input_op:?}"
                    );

                    self.input_batch.push_back(input_context);
                } else if let TraceOpContent::InputIteratorExhausted = &input_op.content {
                    assert!(input_data.is_none(), "at {input_op:?}");
                } else {
                    unreachable!();
                }
            } else {
                break input_op;
            }
        };

        match &next_op.content {
            TraceOpContent::YieldFrom(YieldValue::ResolveCoercion(trace_context, can_coerce)) => {
                let input_context = self.input_batch.pop_front().unwrap();
                assert_eq!(
                    trace_context,
                    &input_context.clone().flat_map(&mut |v| v.into_vertex()),
                    "at {next_op:?}"
                );
                Some((input_context, *can_coerce))
            }
            TraceOpContent::OutputIteratorExhausted => {
                assert!(self.input_batch.pop_front().is_none(), "at {next_op:?}");
                self.exhausted = true;
                None
            }
            _ => unreachable!(),
        }
    }
}

struct TraceReaderResolveNeighborsIter<'query, 'trace, V, Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'query,
    V: AsVertex<Vertex>,
    'trace: 'query,
{
    exhausted: bool,
    parent_opid: Opid,
    contexts: ContextIterator<'query, V>,
    input_batch: VecDeque<DataContext<V>>,
    inner: Rc<RefCell<btree_map::Iter<'trace, Opid, TraceOp<Vertex>>>>,
}

impl<'query, 'trace, V, Vertex> Iterator
    for TraceReaderResolveNeighborsIter<'query, 'trace, V, Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'query,
    V: AsVertex<Vertex>,
    'trace: 'query,
{
    type Item = (DataContext<V>, VertexIterator<'query, Vertex>);

    fn next(&mut self) -> Option<Self::Item> {
        assert!(!self.exhausted);
        let next_op = loop {
            let (_, input_op) = advance_ref_iter(self.inner.as_ref())
                .expect("Expected to have an item but found none.");
            assert_eq!(
                self.parent_opid,
                input_op.parent_opid.expect("Expected an operation with a parent_opid."),
                "Expected parent_opid {:?} did not match operation {:#?}",
                self.parent_opid,
                input_op,
            );

            if let TraceOpContent::AdvanceInputIterator = &input_op.content {
                let input_data = self.contexts.next();

                let (_, input_op) = advance_ref_iter(self.inner.as_ref())
                    .expect("Expected to have an item but found none.");
                assert_eq!(
                    self.parent_opid,
                    input_op.parent_opid.expect("Expected an operation with a parent_opid."),
                    "Expected parent_opid {:?} did not match operation {:#?}",
                    self.parent_opid,
                    input_op,
                );

                if let TraceOpContent::YieldInto(context) = &input_op.content {
                    let input_context = input_data.unwrap();
                    assert_eq!(
                        context,
                        &input_context.clone().flat_map(&mut |v| v.into_vertex()),
                        "at {input_op:?}"
                    );

                    self.input_batch.push_back(input_context);
                } else if let TraceOpContent::InputIteratorExhausted = &input_op.content {
                    assert!(input_data.is_none(), "at {input_op:?}");
                } else {
                    unreachable!();
                }
            } else {
                break input_op;
            }
        };

        match &next_op.content {
            TraceOpContent::YieldFrom(YieldValue::ResolveNeighborsOuter(trace_context)) => {
                let input_context = self.input_batch.pop_front().unwrap();
                assert_eq!(
                    trace_context,
                    &input_context.clone().flat_map(&mut |v| v.into_vertex()),
                    "at {next_op:?}"
                );

                let neighbors = Box::new(TraceReaderNeighborIter {
                    exhausted: false,
                    parent_iterator_opid: next_op.opid,
                    next_index: 0,
                    inner: self.inner.clone(),
                    _phantom: PhantomData,
                });
                Some((input_context, neighbors))
            }
            TraceOpContent::OutputIteratorExhausted => {
                assert!(self.input_batch.pop_front().is_none(), "at {next_op:?}");
                self.exhausted = true;
                None
            }
            _ => unreachable!(),
        }
    }
}

struct TraceReaderNeighborIter<'query, 'trace, Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'query,
    'trace: 'query,
{
    exhausted: bool,
    parent_iterator_opid: Opid,
    next_index: usize,
    inner: Rc<RefCell<btree_map::Iter<'trace, Opid, TraceOp<Vertex>>>>,
    _phantom: PhantomData<&'query ()>,
}

impl<'query, 'trace, Vertex> Iterator for TraceReaderNeighborIter<'query, 'trace, Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'query,
    'trace: 'query,
{
    type Item = Vertex;

    fn next(&mut self) -> Option<Self::Item> {
        let (_, trace_op) = advance_ref_iter(self.inner.as_ref())
            .expect("Expected to have an item but found none.");
        assert!(!self.exhausted);
        assert_eq!(
            self.parent_iterator_opid,
            trace_op.parent_opid.expect("Expected an operation with a parent_opid."),
            "Expected parent_opid {:?} did not match operation {:#?}",
            self.parent_iterator_opid,
            trace_op,
        );

        match &trace_op.content {
            TraceOpContent::OutputIteratorExhausted => {
                self.exhausted = true;
                None
            }
            TraceOpContent::YieldFrom(YieldValue::ResolveNeighborsInner(index, vertex)) => {
                assert_eq!(self.next_index, *index, "at {trace_op:?}");
                self.next_index += 1;
                Some(vertex.clone())
            }
            _ => unreachable!(),
        }
    }
}

#[allow(unused_variables)]
impl<'trace, Vertex> Adapter<'trace> for TraceReaderAdapter<'trace, Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'trace,
{
    type Vertex = Vertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexIterator<'trace, Self::Vertex> {
        let (root_opid, trace_op) = advance_ref_iter(self.next_op.as_ref())
            .expect("Expected a resolve_starting_vertices() call operation, but found none.");
        assert_eq!(None, trace_op.parent_opid);

        if let TraceOpContent::Call(FunctionCall::ResolveStartingVertices(vid)) = trace_op.content {
            assert_eq!(vid, resolve_info.vid());

            Box::new(TraceReaderStartingVerticesIter {
                exhausted: false,
                parent_opid: *root_opid,
                inner: self.next_op.clone(),
            })
        } else {
            unreachable!()
        }
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'trace>(
        &self,
        contexts: ContextIterator<'trace, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'trace, V, FieldValue> {
        let (root_opid, trace_op) = advance_ref_iter(self.next_op.as_ref())
            .expect("Expected a resolve_property() call operation, but found none.");
        assert_eq!(None, trace_op.parent_opid);

        if let TraceOpContent::Call(FunctionCall::ResolveProperty(vid, op_type_name, property)) =
            &trace_op.content
        {
            assert_eq!(*vid, resolve_info.vid());
            assert_eq!(op_type_name, type_name);
            assert_eq!(property, property_name);

            Box::new(TraceReaderResolvePropertiesIter {
                exhausted: false,
                parent_opid: *root_opid,
                contexts,
                input_batch: Default::default(),
                inner: self.next_op.clone(),
            })
        } else {
            unreachable!()
        }
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'trace>(
        &self,
        contexts: ContextIterator<'trace, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'trace, V, VertexIterator<'trace, Self::Vertex>> {
        let (root_opid, trace_op) = advance_ref_iter(self.next_op.as_ref())
            .expect("Expected a resolve_property() call operation, but found none.");
        assert_eq!(None, trace_op.parent_opid);

        if let TraceOpContent::Call(FunctionCall::ResolveNeighbors(vid, op_type_name, eid)) =
            &trace_op.content
        {
            assert_eq!(*vid, resolve_info.origin_vid());
            assert_eq!(op_type_name, type_name);
            assert_eq!(*eid, resolve_info.eid());

            Box::new(TraceReaderResolveNeighborsIter {
                exhausted: false,
                parent_opid: *root_opid,
                contexts,
                input_batch: Default::default(),
                inner: self.next_op.clone(),
            })
        } else {
            unreachable!()
        }
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'trace>(
        &self,
        contexts: ContextIterator<'trace, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'trace, V, bool> {
        let (root_opid, trace_op) = advance_ref_iter(self.next_op.as_ref())
            .expect("Expected a resolve_coercion() call operation, but found none.");
        assert_eq!(None, trace_op.parent_opid);

        if let TraceOpContent::Call(FunctionCall::ResolveCoercion(vid, from_type, to_type)) =
            &trace_op.content
        {
            assert_eq!(*vid, resolve_info.vid());
            assert_eq!(from_type, type_name);
            assert_eq!(to_type, coerce_to_type);

            Box::new(TraceReaderResolveCoercionIter {
                exhausted: false,
                parent_opid: *root_opid,
                contexts,
                input_batch: Default::default(),
                inner: self.next_op.clone(),
            })
        } else {
            unreachable!()
        }
    }
}

#[allow(dead_code)]
pub fn assert_interpreted_results<'query, 'trace, Vertex>(
    trace: &Trace<Vertex>,
    expected_results: &[BTreeMap<Arc<str>, FieldValue>],
    complete: bool,
) where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned + 'query,
    'trace: 'query,
{
    let next_op = Rc::new(RefCell::new(trace.ops.iter()));
    let trace_reader_adapter = Arc::new(TraceReaderAdapter { next_op: next_op.clone() });

    let query: Arc<IndexedQuery> = Arc::new(trace.ir_query.clone().try_into().unwrap());
    let arguments = Arc::new(
        trace.arguments.iter().map(|(k, v)| (Arc::from(k.to_owned()), v.clone())).collect(),
    );
    let mut trace_iter = interpret_ir(trace_reader_adapter, query, arguments).unwrap();
    let mut expected_iter = expected_results.iter();

    loop {
        let expected_row = expected_iter.next();
        let trace_row = trace_iter.next();

        if let Some(expected_row_content) = expected_row {
            let trace_expected_row = {
                let mut next_op_ref = next_op.borrow_mut();
                let Some((_, trace_op)) = next_op_ref.next() else {
                    panic!("Reached the end of the trace without producing result {trace_row:#?}");
                };
                let TraceOpContent::ProduceQueryResult(expected_result) = &trace_op.content else {
                    panic!("Expected the trace to produce a result {trace_row:#?} but got another type of operation instead: {trace_op:#?}");
                };
                drop(next_op_ref);

                expected_result
            };
            assert_eq!(
                trace_expected_row, expected_row_content,
                "This trace is self-inconsistent: trace produces row {trace_expected_row:#?} \
                but results have row {expected_row_content:#?}",
            );

            assert_eq!(expected_row, trace_row.as_ref());
        } else {
            if complete {
                assert_eq!(None, trace_row);
            }
            return;
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

    use serde::{de::DeserializeOwned, Serialize};
    use trustfall_filetests_macros::parameterize;

    use crate::{
        filesystem_interpreter::FilesystemVertex,
        interpreter::replay::assert_interpreted_results,
        numbers_interpreter::NumbersVertex,
        test_types::{
            TestIRQuery, TestIRQueryResult, TestInterpreterOutputData, TestInterpreterOutputTrace,
        },
    };

    fn check_trace<Vertex>(
        expected_ir: TestIRQuery,
        test_data: TestInterpreterOutputTrace<Vertex>,
        test_outputs: TestInterpreterOutputData,
    ) where
        Vertex: Debug + Clone + PartialEq + Eq + Serialize + DeserializeOwned,
    {
        // Ensure that the trace file's IR hasn't drifted away from the IR file of the same name.
        assert_eq!(expected_ir.ir_query, test_data.trace.ir_query);
        assert_eq!(expected_ir.arguments, test_data.trace.arguments);

        assert_interpreted_results(&test_data.trace, &test_outputs.results, true);
    }

    fn check_filesystem_trace(
        expected_ir: TestIRQuery,
        input_data: &str,
        test_outputs: TestInterpreterOutputData,
    ) {
        match ron::from_str::<TestInterpreterOutputTrace<FilesystemVertex>>(input_data) {
            Ok(test_data) => {
                assert_eq!(expected_ir.schema_name, "filesystem");
                assert_eq!(test_data.schema_name, "filesystem");
                check_trace(expected_ir, test_data, test_outputs);
            }
            Err(e) => {
                unreachable!("failed to parse trace file: {e}");
            }
        }
    }

    fn check_numbers_trace(
        expected_ir: TestIRQuery,
        input_data: &str,
        test_outputs: TestInterpreterOutputData,
    ) {
        match ron::from_str::<TestInterpreterOutputTrace<NumbersVertex>>(input_data) {
            Ok(test_data) => {
                assert_eq!(expected_ir.schema_name, "numbers");
                assert_eq!(test_data.schema_name, "numbers");
                check_trace(expected_ir, test_data, test_outputs);
            }
            Err(e) => {
                unreachable!("failed to parse trace file: {e}");
            }
        }
    }

    #[parameterize("trustfall_core/test_data/tests/valid_queries")]
    fn parameterized_tester(base: &Path, stem: &str) {
        let mut input_path = PathBuf::from(base);
        input_path.push(format!("{stem}.trace.ron"));

        let input_data = fs::read_to_string(input_path).unwrap();

        let mut output_data_path = PathBuf::from(base);
        output_data_path.push(format!("{stem}.output.ron"));
        let output_data =
            fs::read_to_string(output_data_path).expect("failed to read outputs file");
        let test_outputs: TestInterpreterOutputData =
            ron::from_str(&output_data).expect("failed to parse outputs file");

        let mut check_path = PathBuf::from(base);
        check_path.push(format!("{stem}.ir.ron"));
        let check_data = fs::read_to_string(check_path).unwrap();
        let expected_ir: TestIRQueryResult = ron::from_str(&check_data).unwrap();
        let expected_ir = expected_ir.unwrap();

        match expected_ir.schema_name.as_str() {
            "filesystem" => check_filesystem_trace(expected_ir, input_data.as_str(), test_outputs),
            "numbers" => check_numbers_trace(expected_ir, input_data.as_str(), test_outputs),
            _ => unreachable!("{}", expected_ir.schema_name),
        }
    }
}
