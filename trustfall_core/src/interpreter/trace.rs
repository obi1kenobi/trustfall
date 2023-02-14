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

use super::{ContextIterator, ContextOutcomeIterator, QueryInfo, VertexIterator};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Opid(pub NonZeroUsize); // operation ID

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "Vertex: Serialize, for<'de2> Vertex: Deserialize<'de2>")]
pub struct Trace<Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize,
    for<'de2> Vertex: Deserialize<'de2>,
{
    pub ops: BTreeMap<Opid, TraceOp<Vertex>>,

    pub ir_query: IRQuery,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) arguments: BTreeMap<String, FieldValue>,
}

impl<Vertex> Trace<Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize,
    for<'de2> Vertex: Deserialize<'de2>,
{
    #[allow(dead_code)]
    pub fn new(ir_query: IRQuery, arguments: BTreeMap<String, FieldValue>) -> Self {
        Self {
            ops: Default::default(),
            ir_query,
            arguments,
        }
    }

    pub fn record(&mut self, content: TraceOpContent<Vertex>, parent: Option<Opid>) -> Opid {
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
#[serde(bound = "Vertex: Serialize, for<'de2> Vertex: Deserialize<'de2>")]
pub struct TraceOp<Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize,
    for<'de2> Vertex: Deserialize<'de2>,
{
    pub opid: Opid,
    pub parent_opid: Option<Opid>, // None parent_opid means this is a top-level operation

    pub content: TraceOpContent<Vertex>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "Vertex: Serialize, for<'de2> Vertex: Deserialize<'de2>")]
pub enum TraceOpContent<Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize,
    for<'de2> Vertex: Deserialize<'de2>,
{
    // TODO: make a way to differentiate between different queries recorded in the same trace
    Call(FunctionCall),

    AdvanceInputIterator,
    YieldInto(DataContext<Vertex>),
    YieldFrom(YieldValue<Vertex>),

    InputIteratorExhausted,
    OutputIteratorExhausted,

    ProduceQueryResult(BTreeMap<Arc<str>, FieldValue>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionCall {
    GetStartingTokens(Vid),                   // vertex ID
    ProjectProperty(Vid, Arc<str>, Arc<str>), // vertex ID + type name + name of the property
    ProjectNeighbors(Vid, Arc<str>, Eid),     // vertex ID + type name + edge ID
    CanCoerceToType(Vid, Arc<str>, Arc<str>), // vertex ID + current type + coerced-to type
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "Vertex: Serialize, for<'de2> Vertex: Deserialize<'de2>")]
pub enum YieldValue<Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize,
    for<'de2> Vertex: Deserialize<'de2>,
{
    GetStartingTokens(Vertex),
    ProjectProperty(DataContext<Vertex>, FieldValue),
    ProjectNeighborsOuter(DataContext<Vertex>),
    ProjectNeighborsInner(usize, Vertex), // iterable index + produced element
    CanCoerceToType(DataContext<Vertex>, bool),
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
pub struct AdapterTap<'vertex, AdapterT>
where
    AdapterT: Adapter<'vertex>,
    AdapterT::Vertex: Clone + Debug + PartialEq + Eq + Serialize + 'vertex,
    for<'de2> AdapterT::Vertex: Deserialize<'de2>,
{
    tracer: Rc<RefCell<Trace<AdapterT::Vertex>>>,
    inner: AdapterT,
    _phantom: PhantomData<&'vertex ()>,
}

impl<'vertex, AdapterT> AdapterTap<'vertex, AdapterT>
where
    AdapterT: Adapter<'vertex>,
    AdapterT::Vertex: Clone + Debug + PartialEq + Eq + Serialize + 'vertex,
    for<'de2> AdapterT::Vertex: Deserialize<'de2>,
{
    pub fn new(adapter: AdapterT, tracer: Rc<RefCell<Trace<AdapterT::Vertex>>>) -> Self {
        Self {
            tracer,
            inner: adapter,
            _phantom: PhantomData,
        }
    }

    pub fn finish(self) -> Trace<AdapterT::Vertex> {
        // Ensure nothing is reading the trace i.e. we can safely stop interpreting.
        let trace_ref = self.tracer.borrow_mut();
        let new_trace = Trace::new(trace_ref.ir_query.clone(), trace_ref.arguments.clone());
        drop(trace_ref);
        self.tracer.replace(new_trace)
    }
}

#[allow(dead_code)]
pub(crate) fn tap_results<'vertex, AdapterT>(
    adapter_tap: Rc<RefCell<AdapterTap<'vertex, AdapterT>>>,
    result_iter: impl Iterator<Item = BTreeMap<Arc<str>, FieldValue>> + 'vertex,
) -> impl Iterator<Item = BTreeMap<Arc<str>, FieldValue>> + 'vertex
where
    AdapterT: Adapter<'vertex> + 'vertex,
    AdapterT::Vertex: Clone + Debug + PartialEq + Eq + Serialize + 'vertex,
    for<'de2> AdapterT::Vertex: Deserialize<'de2>,
{
    result_iter.map(move |result| {
        let adapter_ref = adapter_tap.borrow_mut();
        adapter_ref
            .tracer
            .borrow_mut()
            .record(TraceOpContent::ProduceQueryResult(result.clone()), None);

        result
    })
}

impl<'vertex, AdapterT> Adapter<'vertex> for AdapterTap<'vertex, AdapterT>
where
    AdapterT: Adapter<'vertex>,
    AdapterT::Vertex: Clone + Debug + PartialEq + Eq + Serialize + 'vertex,
    for<'de2> AdapterT::Vertex: Deserialize<'de2>,
{
    type Vertex = AdapterT::Vertex;

    fn resolve_starting_vertices(
        &mut self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        query_info: &QueryInfo,
    ) -> VertexIterator<'vertex, Self::Vertex> {
        let mut trace = self.tracer.borrow_mut();
        let call_opid = trace.record(
            TraceOpContent::Call(FunctionCall::GetStartingTokens(query_info.origin_vid())),
            None,
        );
        drop(trace);

        assert!(query_info.origin_crossing_eid().is_none());

        let inner_iter = self
            .inner
            .resolve_starting_vertices(edge_name, parameters, query_info);
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

    fn resolve_property(
        &mut self,
        contexts: ContextIterator<'vertex, Self::Vertex>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'vertex, Self::Vertex, FieldValue> {
        let mut trace = self.tracer.borrow_mut();
        let call_opid = trace.record(
            TraceOpContent::Call(FunctionCall::ProjectProperty(
                query_info.origin_vid(),
                type_name.clone(),
                property_name.clone(),
            )),
            None,
        );
        drop(trace);

        assert!(query_info.origin_crossing_eid().is_none());

        let tracer_ref_1 = self.tracer.clone();
        let tracer_ref_2 = self.tracer.clone();
        let tracer_ref_3 = self.tracer.clone();
        let wrapped_contexts = Box::new(
            make_iter_with_end_action(
                make_iter_with_pre_action(contexts, move || {
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
        let inner_iter =
            self.inner
                .resolve_property(wrapped_contexts, type_name, property_name, query_info);

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

    fn resolve_neighbors(
        &mut self,
        contexts: ContextIterator<'vertex, Self::Vertex>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'vertex, Self::Vertex, VertexIterator<'vertex, Self::Vertex>> {
        let mut trace = self.tracer.borrow_mut();
        let call_opid = trace.record(
            TraceOpContent::Call(FunctionCall::ProjectNeighbors(
                query_info.origin_vid(),
                type_name.clone(),
                query_info
                    .origin_crossing_eid()
                    .expect("no Eid when projecting neighbors"),
            )),
            None,
        );
        drop(trace);

        let tracer_ref_1 = self.tracer.clone();
        let tracer_ref_2 = self.tracer.clone();
        let tracer_ref_3 = self.tracer.clone();
        let wrapped_contexts = Box::new(
            make_iter_with_end_action(
                make_iter_with_pre_action(contexts, move || {
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
        let inner_iter = self.inner.resolve_neighbors(
            wrapped_contexts,
            type_name,
            edge_name,
            parameters,
            query_info,
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
                let final_neighbor_iter: VertexIterator<'vertex, Self::Vertex> =
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

    fn resolve_coercion(
        &mut self,
        contexts: ContextIterator<'vertex, Self::Vertex>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'vertex, Self::Vertex, bool> {
        let mut trace = self.tracer.borrow_mut();
        let call_opid = trace.record(
            TraceOpContent::Call(FunctionCall::CanCoerceToType(
                query_info.origin_vid(),
                type_name.clone(),
                coerce_to_type.clone(),
            )),
            None,
        );
        drop(trace);

        assert!(query_info.origin_crossing_eid().is_none());

        let tracer_ref_1 = self.tracer.clone();
        let tracer_ref_2 = self.tracer.clone();
        let tracer_ref_3 = self.tracer.clone();
        let wrapped_contexts = Box::new(
            make_iter_with_end_action(
                make_iter_with_pre_action(contexts, move || {
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
        let inner_iter =
            self.inner
                .resolve_coercion(wrapped_contexts, type_name, coerce_to_type, query_info);

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
