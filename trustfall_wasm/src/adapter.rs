use std::{sync::Arc, cell::RefCell, collections::BTreeMap, rc::Rc};

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
#[derive(Debug, Clone)]
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
    registry: Rc<RefCell<BTreeMap<u32, DataContext<JsValue>>>>,
    next_item: u32,
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
        Self {
            iter,
            registry: Rc::from(RefCell::new(Default::default())),
            next_item: 0,
        }
    }

    pub fn advance(&mut self) -> RawIteratorItem {
        let next = self.iter.next();
        if let Some(ctx) = next.clone() {
            let next_item = self.next_item;
            self.next_item = self.next_item.wrapping_add(1);
            let existing = self.registry.borrow_mut().insert(next_item, ctx);
            assert!(existing.is_none(), "id {} already inserted with value {:?}", next_item, existing);
        }
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
    registry: Rc<RefCell<BTreeMap<u32, DataContext<JsValue>>>>,
    next_item: u32,
}

impl ContextAndValueIterator {
    fn new(inner: js_sys::Iterator, registry: Rc<RefCell<BTreeMap<u32, DataContext<JsValue>>>>) -> Self {
        Self { inner, registry, next_item: 0 }
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
            assert!(self.registry.borrow().is_empty());
            None
        } else {
            let next_item = self.next_item;
            self.next_item = self.next_item.wrapping_add(1);
            let ctx = self.registry.borrow_mut().remove(&next_item).expect("id not found");

            let value = iter_next.value();
            let field_value = if value.is_null() {
                FieldValue::Null
            } else if let Some(s) = value.as_string() {
                FieldValue::String(s)
            } else if let Some(f) = value.as_f64() {
                FieldValue::Float64(f)
            } else if let Some(b) = value.as_bool() {
                FieldValue::Boolean(b)
            } else {
                panic!("unhandled value: {:?}", value)
            };

            Some((ctx, field_value))
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct WrappedValue(FieldValue);

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct ContextAndFieldValue {
    context: WrappedContext,
    value: WrappedValue,
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
        let ctx_iter = ContextIterator::new(data_contexts);
        let registry = ctx_iter.registry.clone();
        let js_iter = self.inner.project_property(ctx_iter, current_type_name.as_ref(), field_name.as_ref());
        Box::new(ContextAndValueIterator::new(js_iter, registry))
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
