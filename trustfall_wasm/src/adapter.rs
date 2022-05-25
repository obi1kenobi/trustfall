use std::{sync::Arc, cell::RefCell, collections::BTreeMap, rc::Rc};

use trustfall_core::{
    interpreter::{Adapter, DataContext, InterpretedQuery},
    ir::{EdgeParameters as CoreEdgeParameters, Eid, FieldValue, Vid},
};
use wasm_bindgen::prelude::*;

use crate::shim::{EdgeParameters, ContextIterator, ReturnedContextIdAndValue};

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

            let value = iter_next.value();
            log!("received={:?}", value);

            // let value_str = value.as_string().expect("value was not a string");
            // let next_element: ReturnedContextIdAndValue = serde_json::from_str(value_str.as_str()).expect("serde deserialization failed");

            let next_element: ReturnedContextIdAndValue = value.into_serde().expect("not a legal iterator element");
            assert_eq!(next_element.local_id, next_item);

            self.next_item = self.next_item.wrapping_add(1);

            let ctx = self.registry.borrow_mut().remove(&next_item).expect("id not found");

            Some((ctx, next_element.value.into()))
        }
    }
}

pub(super) struct AdapterShim {
    inner: JsAdapter,
}

impl AdapterShim {
    pub(super) fn new(inner: JsAdapter) -> Self {
        Self { inner }
    }
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
