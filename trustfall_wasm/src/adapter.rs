use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

use gloo_utils::format::JsValueSerdeExt;
use js_sys::try_iter;
use trustfall_core::{
    interpreter::{
        Adapter, ContextIterator, ContextOutcomeIterator, DataContext, QueryInfo, VertexIterator, QueryInfoAlongEdge,
    },
    ir::{EdgeParameters as CoreEdgeParameters, FieldValue},
};
use wasm_bindgen::prelude::*;

use crate::shim::{
    JsContextIterator, JsEdgeParameters, JsStringConstants, ReturnedContextIdAndBool,
    ReturnedContextIdAndValue,
};

#[wasm_bindgen]
extern "C" {
    pub type JsAdapter;

    #[wasm_bindgen(structural, method, js_name = "resolveStartingVertices")]
    pub fn resolve_starting_vertices(
        this: &JsAdapter,
        edge: &str,
        parameters: JsValue,
    ) -> js_sys::Iterator;

    #[wasm_bindgen(structural, method, js_name = "resolveProperty")]
    pub fn resolve_property(
        this: &JsAdapter,
        contexts: JsContextIterator,
        type_name: &str,
        field_name: &str,
    ) -> js_sys::Iterator;

    #[wasm_bindgen(structural, method, js_name = "resolveNeighbors")]
    pub fn resolve_neighbors(
        this: &JsAdapter,
        contexts: JsContextIterator,
        type_name: &str,
        edge_name: &str,
        parameters: JsValue,
    ) -> js_sys::Iterator;

    #[wasm_bindgen(structural, method, js_name = "resolveCoercion")]
    pub fn resolve_coercion(
        this: &JsAdapter,
        contexts: JsContextIterator,
        type_name: &str,
        coerce_to_type: &str,
    ) -> js_sys::Iterator;
}

struct JsVertexIterator {
    inner: js_sys::IntoIter,
}

impl JsVertexIterator {
    fn new(inner: js_sys::IntoIter) -> Self {
        Self { inner }
    }
}

impl Iterator for JsVertexIterator {
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
    type Item = (DataContext<JsValue>, VertexIterator<'static, JsValue>);

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

            Some((ctx, Box::new(JsVertexIterator::new(neighbors_iter))))
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

impl Adapter<'static> for AdapterShim {
    type Vertex = JsValue;

    fn resolve_starting_vertices(
        &mut self,
        edge_name: &Arc<str>,
        parameters: &CoreEdgeParameters,
        _query_info: &QueryInfo,
    ) -> VertexIterator<'static, Self::Vertex> {
        let parameters: JsEdgeParameters = parameters.clone().into();
        let js_iter = self
            .inner
            .resolve_starting_vertices(edge_name.as_ref(), parameters.into_js_dict());
        Box::new(JsVertexIterator::new(js_iter.into_iter()))
    }

    fn resolve_property(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        _query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
        let ctx_iter = JsContextIterator::new(contexts);
        let registry = ctx_iter.registry.clone();
        let js_iter =
            self.inner
                .resolve_property(ctx_iter, type_name.as_ref(), property_name.as_ref());
        Box::new(ContextAndValueIterator::new(js_iter, registry))
    }

    fn resolve_neighbors(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &CoreEdgeParameters,
        _query_info: &QueryInfoAlongEdge,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>> {
        let ctx_iter = JsContextIterator::new(contexts);
        let registry = ctx_iter.registry.clone();
        let parameters: JsEdgeParameters = parameters.clone().into();

        let js_iter = self.inner.resolve_neighbors(
            ctx_iter,
            type_name.as_ref(),
            edge_name.as_ref(),
            parameters.into_js_dict(),
        );
        Box::new(ContextAndNeighborsIterator::new(
            js_iter,
            registry,
            self.constants.clone(),
        ))
    }

    fn resolve_coercion(
        &mut self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        _query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
        let ctx_iter = JsContextIterator::new(contexts);
        let registry = ctx_iter.registry.clone();
        let js_iter =
            self.inner
                .resolve_coercion(ctx_iter, type_name.as_ref(), coerce_to_type.as_ref());
        Box::new(ContextAndBoolIterator::new(js_iter, registry))
    }
}
