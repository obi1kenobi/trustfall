use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

use js_sys::try_iter;
use trustfall_core::{
    interpreter::{Adapter, DataContext, InterpretedQuery},
    ir::{EdgeParameters as CoreEdgeParameters, Eid, FieldValue, Vid},
};
use wasm_bindgen::prelude::*;

use crate::shim::{
    ContextIterator, JsEdgeParameters, JsStringConstants, ReturnedContextIdAndBool,
    ReturnedContextIdAndValue,
};

#[wasm_bindgen]
extern "C" {
    pub type JsAdapter;

    #[wasm_bindgen(structural, method, js_name = "getStartingTokens")]
    pub fn get_starting_tokens(
        this: &JsAdapter,
        edge: &str,
        parameters: JsValue,
    ) -> js_sys::Iterator;

    #[wasm_bindgen(structural, method, js_name = "projectProperty")]
    pub fn project_property(
        this: &JsAdapter,
        data_contexts: ContextIterator,
        current_type_name: &str,
        field_name: &str,
    ) -> js_sys::Iterator;

    #[wasm_bindgen(structural, method, js_name = "projectNeighbors")]
    pub fn project_neighbors(
        this: &JsAdapter,
        data_contexts: ContextIterator,
        current_type_name: &str,
        edge_name: &str,
        parameters: JsValue,
    ) -> js_sys::Iterator;

    #[wasm_bindgen(structural, method, js_name = "canCoerceToType")]
    pub fn can_coerce_to_type(
        this: &JsAdapter,
        data_contexts: ContextIterator,
        current_type_name: &str,
        coerce_to_type_name: &str,
    ) -> js_sys::Iterator;
}

struct TokenIterator {
    inner: js_sys::IntoIter,
}

impl TokenIterator {
    fn new(inner: js_sys::IntoIter) -> Self {
        Self { inner }
    }
}

impl Iterator for TokenIterator {
    type Item = JsValue;

    fn next(&mut self) -> Option<Self::Item> {
        let next_value = self
            .inner
            .next()?
            .expect("unexpected value returned from JS iterator next()");

        Some(next_value)
    }
}

struct ContextAndValueIterator {
    inner: js_sys::Iterator,
    registry: Rc<RefCell<BTreeMap<u32, DataContext<JsValue>>>>,
    next_item: u32,
}

impl ContextAndValueIterator {
    fn new(
        inner: js_sys::Iterator,
        registry: Rc<RefCell<BTreeMap<u32, DataContext<JsValue>>>>,
    ) -> Self {
        Self {
            inner,
            registry,
            next_item: 0,
        }
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

            let next_element: ReturnedContextIdAndValue =
                value.into_serde().expect("not a legal iterator element");
            assert_eq!(next_element.local_id, next_item);

            self.next_item = self.next_item.wrapping_add(1);

            let ctx = self
                .registry
                .borrow_mut()
                .remove(&next_item)
                .expect("id not found");

            Some((ctx, next_element.value.into()))
        }
    }
}

struct ContextAndNeighborsIterator {
    inner: js_sys::Iterator,
    registry: Rc<RefCell<BTreeMap<u32, DataContext<JsValue>>>>,
    next_item: u32,
    constants: Rc<JsStringConstants>,
}

impl ContextAndNeighborsIterator {
    fn new(
        inner: js_sys::Iterator,
        registry: Rc<RefCell<BTreeMap<u32, DataContext<JsValue>>>>,
        constants: Rc<JsStringConstants>,
    ) -> Self {
        Self {
            inner,
            registry,
            next_item: 0,
            constants,
        }
    }
}

impl Iterator for ContextAndNeighborsIterator {
    type Item = (
        DataContext<JsValue>,
        Box<dyn Iterator<Item = JsValue> + 'static>,
    );

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

            let local_id = js_sys::Reflect::get(&value, &self.constants.local_id)
                .expect("could not retrieve target.localId value");
            let neighbors_value = js_sys::Reflect::get(&value, &self.constants.neighbors)
                .expect("could not retrieve target.neighbors value");
            let neighbors_iter = try_iter(&neighbors_value)
                .expect("attempting to look up Symbol.iterator threw an exception")
                .expect("element neighbors value was not an iterator");

            assert_eq!(local_id, next_item);

            self.next_item = self.next_item.wrapping_add(1);

            let ctx = self
                .registry
                .borrow_mut()
                .remove(&next_item)
                .expect("id not found");

            Some((ctx, Box::new(TokenIterator::new(neighbors_iter))))
        }
    }
}

struct ContextAndBoolIterator {
    inner: js_sys::Iterator,
    registry: Rc<RefCell<BTreeMap<u32, DataContext<JsValue>>>>,
    next_item: u32,
}

impl ContextAndBoolIterator {
    fn new(
        inner: js_sys::Iterator,
        registry: Rc<RefCell<BTreeMap<u32, DataContext<JsValue>>>>,
    ) -> Self {
        Self {
            inner,
            registry,
            next_item: 0,
        }
    }
}

impl Iterator for ContextAndBoolIterator {
    type Item = (DataContext<JsValue>, bool);

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

            let next_element: ReturnedContextIdAndBool =
                value.into_serde().expect("not a legal iterator element");
            assert_eq!(next_element.local_id, next_item);

            self.next_item = self.next_item.wrapping_add(1);

            let ctx = self
                .registry
                .borrow_mut()
                .remove(&next_item)
                .expect("id not found");

            Some((ctx, next_element.value))
        }
    }
}

pub struct AdapterShim {
    inner: JsAdapter,
    constants: Rc<JsStringConstants>,
}

impl AdapterShim {
    pub fn new(inner: JsAdapter) -> Self {
        Self {
            inner,
            constants: Rc::new(JsStringConstants::new()),
        }
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
        let parameters: JsEdgeParameters = parameters.into();
        let js_iter = self
            .inner
            .get_starting_tokens(edge.as_ref(), parameters.into_js_dict());
        Box::new(TokenIterator::new(js_iter.into_iter()))
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
        let js_iter =
            self.inner
                .project_property(ctx_iter, current_type_name.as_ref(), field_name.as_ref());
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
        let ctx_iter = ContextIterator::new(data_contexts);
        let registry = ctx_iter.registry.clone();
        let parameters: JsEdgeParameters = parameters.into();

        let js_iter = self.inner.project_neighbors(
            ctx_iter,
            current_type_name.as_ref(),
            edge_name.as_ref(),
            parameters.into_js_dict(),
        );
        Box::new(ContextAndNeighborsIterator::new(
            js_iter,
            registry,
            self.constants.clone(),
        ))
    }

    fn can_coerce_to_type(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'static>,
        current_type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, bool)> + 'static> {
        let ctx_iter = ContextIterator::new(data_contexts);
        let registry = ctx_iter.registry.clone();
        let js_iter = self.inner.can_coerce_to_type(
            ctx_iter,
            current_type_name.as_ref(),
            coerce_to_type_name.as_ref(),
        );
        Box::new(ContextAndBoolIterator::new(js_iter, registry))
    }
}
