use std::{
    cell::RefCell, collections::BTreeMap, fmt::Debug, marker::PhantomData, num::NonZeroUsize,
    rc::Rc, sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    interpreter::{Adapter, DataContext},
    ir::{EdgeParameters, Eid, FieldValue, IRQuery, Vid},
    util::BTreeMapTryInsertExt,
};

use super::InterpretedQuery;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Opid(pub NonZeroUsize); // operation ID

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "DataToken: Serialize, for<'de2> DataToken: Deserialize<'de2>")]
pub struct Trace<DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize,
    for<'de2> DataToken: Deserialize<'de2>,
{
    pub ops: BTreeMap<Opid, TraceOp<DataToken>>,

    pub ir_query: IRQuery,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) arguments: BTreeMap<String, FieldValue>,
}

impl<DataToken> Trace<DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize,
    for<'de2> DataToken: Deserialize<'de2>,
{
    #[allow(dead_code)]
    pub fn new(ir_query: IRQuery, arguments: BTreeMap<String, FieldValue>) -> Self {
        Self {
            ops: Default::default(),
            ir_query,
            arguments,
        }
    }

    pub fn record(&mut self, content: TraceOpContent<DataToken>, parent: Option<Opid>) -> Opid {
        let next_opid = Opid(NonZeroUsize::new(self.ops.len() + 1).unwrap());

        let op = TraceOp {
            opid: next_opid,
            parent_opid: parent,
            content,
        };
        self.ops.insert_or_error(next_opid, op).unwrap();
        next_opid
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "DataToken: Serialize, for<'de2> DataToken: Deserialize<'de2>")]
pub struct TraceOp<DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize,
    for<'de2> DataToken: Deserialize<'de2>,
{
    pub opid: Opid,
    pub parent_opid: Option<Opid>, // None parent_opid means this is a top-level operation

    pub content: TraceOpContent<DataToken>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "DataToken: Serialize, for<'de2> DataToken: Deserialize<'de2>")]
pub enum TraceOpContent<DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize,
    for<'de2> DataToken: Deserialize<'de2>,
{
    // TODO: make a way to differentiate between different queries recorded in the same trace
    Call(FunctionCall),

    AdvanceInputIterator,
    YieldInto(DataContext<DataToken>),
    YieldFrom(YieldValue<DataToken>),

    InputIteratorExhausted,
    OutputIteratorExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionCall {
    GetStartingTokens(Vid),                   // vertex ID
    ProjectProperty(Vid, Arc<str>, Arc<str>), // vertex ID + type name + name of the property
    ProjectNeighbors(Vid, Arc<str>, Eid),     // vertex ID + type name + edge ID
    CanCoerceToType(Vid, Arc<str>, Arc<str>), // vertex ID + current type + coerced-to type
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "DataToken: Serialize, for<'de2> DataToken: Deserialize<'de2>")]
pub enum YieldValue<DataToken>
where
    DataToken: Clone + Debug + PartialEq + Eq + Serialize,
    for<'de2> DataToken: Deserialize<'de2>,
{
    GetStartingTokens(DataToken),
    ProjectProperty(DataContext<DataToken>, FieldValue),
    ProjectNeighborsOuter(DataContext<DataToken>),
    ProjectNeighborsInner(usize, DataToken), // iterable index + produced element
    CanCoerceToType(DataContext<DataToken>, bool),
}

pub struct OnIterEnd<T, I: Iterator<Item = T>, F: FnOnce()> {
    inner: I,
    on_end_func: Option<F>,
}

impl<T, I, F> Iterator for OnIterEnd<T, I, F>
where
    F: FnOnce(),
    I: Iterator<Item = T>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.inner.next();
        if result.is_none() {
            let end_func = self.on_end_func.take();
            if let Some(func) = end_func {
                func();
            }
        }
        result
    }
}

fn make_iter_with_end_action<T, I: Iterator<Item = T>, F: FnOnce()>(
    inner: I,
    on_end: F,
) -> OnIterEnd<T, I, F> {
    OnIterEnd {
        inner,
        on_end_func: Some(on_end),
    }
}

pub struct PreActionIter<T, I: Iterator<Item = T>, F: Fn()> {
    inner: I,
    pre_action: F,
}

impl<T, I, F> Iterator for PreActionIter<T, I, F>
where
    F: Fn(),
    I: Iterator<Item = T>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        (self.pre_action)();
        self.inner.next()
    }
}

fn make_iter_with_pre_action<T, I: Iterator<Item = T>, F: Fn()>(
    inner: I,
    pre_action: F,
) -> PreActionIter<T, I, F> {
    PreActionIter { inner, pre_action }
}

#[derive(Debug, Clone)]
pub struct AdapterTap<'token, DataToken, AdapterT>
where
    AdapterT: Adapter<'token, DataToken = DataToken>,
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'token,
    for<'de2> DataToken: Deserialize<'de2>,
{
    tracer: Rc<RefCell<Trace<DataToken>>>,
    inner: AdapterT,
    _phantom: PhantomData<&'token ()>,
}

impl<'token, DataToken, AdapterT> AdapterTap<'token, DataToken, AdapterT>
where
    AdapterT: Adapter<'token, DataToken = DataToken>,
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'token,
    for<'de2> DataToken: Deserialize<'de2>,
{
    pub fn new(adapter: AdapterT, tracer: Rc<RefCell<Trace<DataToken>>>) -> Self {
        Self {
            tracer,
            inner: adapter,
            _phantom: PhantomData,
        }
    }

    pub fn finish(self) -> Trace<DataToken> {
        // Ensure nothing is reading the trace i.e. we can safely stop interpreting.
        let trace_ref = self.tracer.borrow_mut();
        let new_trace = Trace::new(trace_ref.ir_query.clone(), trace_ref.arguments.clone());
        drop(trace_ref);
        self.tracer.replace(new_trace)
    }
}

impl<'token, DataToken, AdapterT> Adapter<'token> for AdapterTap<'token, DataToken, AdapterT>
where
    AdapterT: Adapter<'token, DataToken = DataToken>,
    DataToken: Clone + Debug + PartialEq + Eq + Serialize + 'token,
    for<'de2> DataToken: Deserialize<'de2>,
{
    type DataToken = DataToken;

    fn get_starting_tokens(
        &mut self,
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken> + 'token> {
        let mut trace = self.tracer.borrow_mut();
        let call_opid = trace.record(
            TraceOpContent::Call(FunctionCall::GetStartingTokens(vertex_hint)),
            None,
        );
        drop(trace);

        let inner_iter =
            self.inner
                .get_starting_tokens(edge_name, parameters, query_hint, vertex_hint);
        let tracer_ref_1 = self.tracer.clone();
        let tracer_ref_2 = self.tracer.clone();
        Box::new(
            make_iter_with_end_action(inner_iter, move || {
                tracer_ref_1
                    .borrow_mut()
                    .record(TraceOpContent::OutputIteratorExhausted, Some(call_opid));
            })
            .map(move |token| {
                tracer_ref_2.borrow_mut().record(
                    TraceOpContent::YieldFrom(YieldValue::GetStartingTokens(token.clone())),
                    Some(call_opid),
                );

                token
            }),
        )
    }

    fn project_property(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'token>,
        current_type_name: Arc<str>,
        field_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, FieldValue)> + 'token> {
        let mut trace = self.tracer.borrow_mut();
        let call_opid = trace.record(
            TraceOpContent::Call(FunctionCall::ProjectProperty(
                vertex_hint,
                current_type_name.clone(),
                field_name.clone(),
            )),
            None,
        );
        drop(trace);

        let tracer_ref_1 = self.tracer.clone();
        let tracer_ref_2 = self.tracer.clone();
        let tracer_ref_3 = self.tracer.clone();
        let wrapped_contexts = Box::new(
            make_iter_with_end_action(
                make_iter_with_pre_action(data_contexts, move || {
                    tracer_ref_1
                        .borrow_mut()
                        .record(TraceOpContent::AdvanceInputIterator, Some(call_opid));
                }),
                move || {
                    tracer_ref_2
                        .borrow_mut()
                        .record(TraceOpContent::InputIteratorExhausted, Some(call_opid));
                },
            )
            .map(move |context| {
                tracer_ref_3
                    .borrow_mut()
                    .record(TraceOpContent::YieldInto(context.clone()), Some(call_opid));
                context
            }),
        );
        let inner_iter = self.inner.project_property(
            wrapped_contexts,
            current_type_name,
            field_name,
            query_hint,
            vertex_hint,
        );

        let tracer_ref_4 = self.tracer.clone();
        let tracer_ref_5 = self.tracer.clone();
        Box::new(
            make_iter_with_end_action(inner_iter, move || {
                tracer_ref_4
                    .borrow_mut()
                    .record(TraceOpContent::OutputIteratorExhausted, Some(call_opid));
            })
            .map(move |(context, value)| {
                tracer_ref_5.borrow_mut().record(
                    TraceOpContent::YieldFrom(YieldValue::ProjectProperty(
                        context.clone(),
                        value.clone(),
                    )),
                    Some(call_opid),
                );

                (context, value)
            }),
        )
    }

    #[allow(clippy::type_complexity)]
    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'token>,
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
                    Box<dyn Iterator<Item = Self::DataToken> + 'token>,
                ),
            > + 'token,
    > {
        let mut trace = self.tracer.borrow_mut();
        let call_opid = trace.record(
            TraceOpContent::Call(FunctionCall::ProjectNeighbors(
                vertex_hint,
                current_type_name.clone(),
                edge_hint,
            )),
            None,
        );
        drop(trace);

        let tracer_ref_1 = self.tracer.clone();
        let tracer_ref_2 = self.tracer.clone();
        let tracer_ref_3 = self.tracer.clone();
        let wrapped_contexts = Box::new(
            make_iter_with_end_action(
                make_iter_with_pre_action(data_contexts, move || {
                    tracer_ref_1
                        .borrow_mut()
                        .record(TraceOpContent::AdvanceInputIterator, Some(call_opid));
                }),
                move || {
                    tracer_ref_2
                        .borrow_mut()
                        .record(TraceOpContent::InputIteratorExhausted, Some(call_opid));
                },
            )
            .map(move |context| {
                tracer_ref_3
                    .borrow_mut()
                    .record(TraceOpContent::YieldInto(context.clone()), Some(call_opid));
                context
            }),
        );
        let inner_iter = self.inner.project_neighbors(
            wrapped_contexts,
            current_type_name,
            edge_name,
            parameters,
            query_hint,
            vertex_hint,
            edge_hint,
        );

        let tracer_ref_4 = self.tracer.clone();
        let tracer_ref_5 = self.tracer.clone();
        Box::new(
            make_iter_with_end_action(inner_iter, move || {
                tracer_ref_4
                    .borrow_mut()
                    .record(TraceOpContent::OutputIteratorExhausted, Some(call_opid));
            })
            .map(move |(context, neighbor_iter)| {
                let mut trace = tracer_ref_5.borrow_mut();
                let outer_iterator_opid = trace.record(
                    TraceOpContent::YieldFrom(YieldValue::ProjectNeighborsOuter(context.clone())),
                    Some(call_opid),
                );
                drop(trace);

                let tracer_ref_6 = tracer_ref_5.clone();
                let tapped_neighbor_iter = neighbor_iter.enumerate().map(move |(pos, token)| {
                    tracer_ref_6.borrow_mut().record(
                        TraceOpContent::YieldFrom(YieldValue::ProjectNeighborsInner(
                            pos,
                            token.clone(),
                        )),
                        Some(outer_iterator_opid),
                    );

                    token
                });

                let tracer_ref_7 = tracer_ref_5.clone();
                let final_neighbor_iter: Box<dyn Iterator<Item = DataToken> + 'token> =
                    Box::new(make_iter_with_end_action(tapped_neighbor_iter, move || {
                        tracer_ref_7.borrow_mut().record(
                            TraceOpContent::OutputIteratorExhausted,
                            Some(outer_iterator_opid),
                        );
                    }));

                (context, final_neighbor_iter)
            }),
        )
    }

    fn can_coerce_to_type(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'token>,
        current_type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, bool)> + 'token> {
        let mut trace = self.tracer.borrow_mut();
        let call_opid = trace.record(
            TraceOpContent::Call(FunctionCall::CanCoerceToType(
                vertex_hint,
                current_type_name.clone(),
                coerce_to_type_name.clone(),
            )),
            None,
        );
        drop(trace);

        let tracer_ref_1 = self.tracer.clone();
        let tracer_ref_2 = self.tracer.clone();
        let tracer_ref_3 = self.tracer.clone();
        let wrapped_contexts = Box::new(
            make_iter_with_end_action(
                make_iter_with_pre_action(data_contexts, move || {
                    tracer_ref_1
                        .borrow_mut()
                        .record(TraceOpContent::AdvanceInputIterator, Some(call_opid));
                }),
                move || {
                    tracer_ref_2
                        .borrow_mut()
                        .record(TraceOpContent::InputIteratorExhausted, Some(call_opid));
                },
            )
            .map(move |context| {
                tracer_ref_3
                    .borrow_mut()
                    .record(TraceOpContent::YieldInto(context.clone()), Some(call_opid));
                context
            }),
        );
        let inner_iter = self.inner.can_coerce_to_type(
            wrapped_contexts,
            current_type_name,
            coerce_to_type_name,
            query_hint,
            vertex_hint,
        );

        let tracer_ref_4 = self.tracer.clone();
        let tracer_ref_5 = self.tracer.clone();
        Box::new(
            make_iter_with_end_action(inner_iter, move || {
                tracer_ref_4
                    .borrow_mut()
                    .record(TraceOpContent::OutputIteratorExhausted, Some(call_opid));
            })
            .map(move |(context, can_coerce)| {
                tracer_ref_5.borrow_mut().record(
                    TraceOpContent::YieldFrom(YieldValue::CanCoerceToType(
                        context.clone(),
                        can_coerce,
                    )),
                    Some(call_opid),
                );

                (context, can_coerce)
            }),
        )
    }
}
