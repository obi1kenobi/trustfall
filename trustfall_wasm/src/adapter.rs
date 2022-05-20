use std::sync::Arc;

use trustfall_core::{
    interpreter::{Adapter, DataContext, InterpretedQuery},
    ir::{EdgeParameters as CoreEdgeParameters, Eid, FieldValue, Vid},
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    pub type JsAdapter;

    #[wasm_bindgen(structural, method)]
    pub fn get_starting_tokens(this: &JsAdapter, edge: &str) -> js_sys::Iterator;

    #[wasm_bindgen(structural, method)]
    pub fn project_property(
        this: &JsAdapter,
        data_contexts: ContextIterator,
        current_type_name: &str,
        field_name: &str,
    ) -> js_sys::Iterator;

    #[wasm_bindgen(structural, method)]
    pub fn project_neighbors(
        this: &JsAdapter,
        data_contexts: ContextIterator,
        current_type_name: &str,
        edge_name: &str,
        parameters: Option<EdgeParameters>,
    ) -> js_sys::Iterator;

    #[wasm_bindgen(structural, method)]
    pub fn can_coerce_to_type(
        this: &JsAdapter,
        data_contexts: ContextIterator,
        current_type_name: &str,
        coerce_to_type_name: &str,
    ) -> js_sys::Iterator;
}

#[wasm_bindgen]
pub struct EdgeParameters {}  // TODO

#[wasm_bindgen]
#[derive(Clone)]
pub struct WrappedContext(DataContext<JsValue>);

#[wasm_bindgen]
impl WrappedContext {
    pub fn current_token(&self) -> JsValue {
        match self.0.current_token.as_ref() {
            None => JsValue::NULL,
            Some(v) => v.clone()
        }
    }
}

#[wasm_bindgen]
pub struct ContextIterator {
    iter: Box<dyn Iterator<Item = DataContext<JsValue>>>,
}

#[wasm_bindgen]
pub struct RawIteratorItem {
    value: Option<WrappedContext>,
}

#[wasm_bindgen]
impl RawIteratorItem {
    fn new(inner: Option<DataContext<JsValue>>) -> Self {
        Self { value: inner.map(WrappedContext) }
    }

    pub fn done(&self) -> bool {
        self.value.is_none()
    }

    pub fn value(&self) -> Option<WrappedContext> {
        self.value.clone()
    }
}

#[wasm_bindgen]
impl ContextIterator {
    fn new(iter: Box<dyn Iterator<Item = DataContext<JsValue>>>) -> Self {
        Self { iter }
    }

    pub fn advance(&mut self) -> RawIteratorItem {
        let next = self.iter.next();
        RawIteratorItem::new(next)
    }
}

struct TokenIterator {
    inner: js_sys::Iterator,
}

impl TokenIterator {
    fn new(inner: js_sys::Iterator) -> Self {
        Self { inner }
    }
}

impl Iterator for TokenIterator {
    type Item = JsValue;

    fn next(&mut self) -> Option<Self::Item> {
        let iter_next = self
            .inner
            .next()
            .expect("unexpected value returned from JS iterator next()");

        if iter_next.done() {
            None
        } else {
            Some(iter_next.value())
        }
    }
}

struct ContextAndValueIterator {
    inner: js_sys::Iterator,
}

impl ContextAndValueIterator {
    fn new(inner: js_sys::Iterator) -> Self {
        Self { inner }
    }
}

impl Iterator for ContextAndValueIterator {
    type Item = (DataContext<JsValue>, FieldValue);

    fn next(&mut self) -> Option<Self::Item> {
        let iter_next = self
            .inner
            .next()
            .expect("unexpected value returned from JS iterator next()");

        if iter_next.done() {
            None
        } else {
            let value = iter_next.value();
            Some(todo!())
        }
    }
}

struct AdapterShim {
    inner: JsAdapter,
}

#[allow(unused_variables)]
impl Adapter<'static> for AdapterShim {
    type DataToken = JsValue;

    fn get_starting_tokens(
        &mut self,
        edge: Arc<str>,
        parameters: Option<Arc<CoreEdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken> + 'static> {
        let js_iter = self.inner.get_starting_tokens(edge.as_ref());
        Box::new(TokenIterator::new(js_iter))
    }

    fn project_property(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'static>,
        current_type_name: Arc<str>,
        field_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, FieldValue)> + 'static> {
        let js_iter = self.inner.project_property(ContextIterator::new(data_contexts), current_type_name.as_ref(), field_name.as_ref());
        Box::new(ContextAndValueIterator::new(js_iter))
    }

    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'static>,
        current_type_name: Arc<str>,
        edge_name: Arc<str>,
        parameters: Option<Arc<CoreEdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
        edge_hint: Eid,
    ) -> Box<
        dyn Iterator<
                Item = (
                    DataContext<Self::DataToken>,
                    Box<dyn Iterator<Item = Self::DataToken> + 'static>,
                ),
            > + 'static,
    > {
        todo!()
    }

    fn can_coerce_to_type(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'static>,
        current_type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, bool)> + 'static> {
        todo!()
    }
}
