use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

use gloo_utils::format::JsValueSerdeExt;
use js_sys::try_iter;
use trustfall_core::{
    interpreter::{
        Adapter, AsVertex, ContextIterator, ContextOutcomeIterator, DataContext, ResolveEdgeInfo,
        ResolveInfo, VertexIterator,
    },
    ir::{EdgeParameters as CoreEdgeParameters, FieldValue},
};
use wasm_bindgen::prelude::*;

use crate::shim::{
    JsContextIterator, JsEdgeParameters, JsStringConstants, ReturnedContextIdAndBool,
    ReturnedContextIdAndValue,
};

#[wasm_bindgen(module = "/js/dist/web_test_query.js")]
extern "C" {
    #[wasm_bindgen(js_name = "testQuery")]
    pub fn js_test_query();
}

#[wasm_bindgen(module = "/js/dist/js_numbers_adapter.js")]
extern "C" {
    pub type JsAdapter;

    #[wasm_bindgen(constructor)]
    pub fn new() -> JsAdapter;

    #[wasm_bindgen(method, js_name = "resolveStartingVertices")]
    pub fn resolve_starting_vertices(
        this: &JsAdapter,
        edge: &str,
        parameters: JsValue,
    ) -> js_sys::Iterator;

    #[wasm_bindgen(method, js_name = "resolveProperty")]
    pub fn resolve_property(
        this: &JsAdapter,
        contexts: JsContextIterator,
        type_name: &str,
        field_name: &str,
    ) -> js_sys::Iterator;

    #[wasm_bindgen(method, js_name = "resolveNeighbors")]
    pub fn resolve_neighbors(
        this: &JsAdapter,
        contexts: JsContextIterator,
        type_name: &str,
        edge_name: &str,
        parameters: JsValue,
    ) -> js_sys::Iterator;

    #[wasm_bindgen(method, js_name = "resolveCoercion")]
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
        let next_value =
            self.inner.next()?.expect("unexpected value returned from JS iterator next()");

        Some(next_value)
    }
}

struct ContextAndValueIterator {
    inner: js_sys::Iterator,
    registry: Rc<RefCell<BTreeMap<u32, Opaque>>>,
    next_item: u32,
}

impl ContextAndValueIterator {
    fn new(inner: js_sys::Iterator, registry: Rc<RefCell<BTreeMap<u32, Opaque>>>) -> Self {
        Self { inner, registry, next_item: 0 }
    }
}

impl Iterator for ContextAndValueIterator {
    type Item = (Opaque, FieldValue);

    fn next(&mut self) -> Option<Self::Item> {
        let iter_next =
            self.inner.next().expect("unexpected value returned from JS iterator next()");

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

            let ctx = self.registry.borrow_mut().remove(&next_item).expect("id not found");

            Some((ctx, next_element.value.into()))
        }
    }
}

struct ContextAndNeighborsIterator {
    inner: js_sys::Iterator,
    registry: Rc<RefCell<BTreeMap<u32, Opaque>>>,
    next_item: u32,
    constants: Rc<JsStringConstants>,
}

impl ContextAndNeighborsIterator {
    fn new(
        inner: js_sys::Iterator,
        registry: Rc<RefCell<BTreeMap<u32, Opaque>>>,
        constants: Rc<JsStringConstants>,
    ) -> Self {
        Self { inner, registry, next_item: 0, constants }
    }
}

impl Iterator for ContextAndNeighborsIterator {
    type Item = (Opaque, VertexIterator<'static, JsValue>);

    fn next(&mut self) -> Option<Self::Item> {
        let iter_next =
            self.inner.next().expect("unexpected value returned from JS iterator next()");

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

            let ctx = self.registry.borrow_mut().remove(&next_item).expect("id not found");

            Some((ctx, Box::new(JsVertexIterator::new(neighbors_iter))))
        }
    }
}

struct ContextAndBoolIterator {
    inner: js_sys::Iterator,
    registry: Rc<RefCell<BTreeMap<u32, Opaque>>>,
    next_item: u32,
}

impl ContextAndBoolIterator {
    fn new(inner: js_sys::Iterator, registry: Rc<RefCell<BTreeMap<u32, Opaque>>>) -> Self {
        Self { inner, registry, next_item: 0 }
    }
}

impl Iterator for ContextAndBoolIterator {
    type Item = (Opaque, bool);

    fn next(&mut self) -> Option<Self::Item> {
        let iter_next =
            self.inner.next().expect("unexpected value returned from JS iterator next()");

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

            let ctx = self.registry.borrow_mut().remove(&next_item).expect("id not found");

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
        Self { inner, constants: Rc::new(JsStringConstants::new()) }
    }
}

#[derive(Debug)]
pub(crate) struct Opaque {
    data: *mut (),
    pub(crate) vertex: Option<JsValue>,
}

impl Opaque {
    fn new<V: AsVertex<JsValue> + 'static>(ctx: DataContext<V>) -> Self {
        let vertex = ctx.active_vertex::<JsValue>().cloned();
        let boxed = Box::new(ctx);
        let data = Box::into_raw(boxed) as *mut ();

        Self { data, vertex }
    }

    /// Converts an `Opaque` into the `DataContext<V>` it points to.
    ///
    /// # Safety
    ///
    /// When an `Opaque` is constructed, it does not store the value of the `V` generic parameter
    /// it was constructed with. The caller of this function must ensure that the `V` parameter here
    /// is the same type as the one used in the `Opaque::new()` call that constructed `self` here.
    unsafe fn into_inner<V: AsVertex<JsValue> + 'static>(self) -> DataContext<V> {
        let boxed_ctx = unsafe { Box::from_raw(self.data as *mut DataContext<V>) };
        *boxed_ctx
    }
}

impl Adapter<'static> for AdapterShim {
    type Vertex = JsValue;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &CoreEdgeParameters,
        _resolve_info: &ResolveInfo,
    ) -> VertexIterator<'static, Self::Vertex> {
        let parameters: JsEdgeParameters = parameters.clone().into();
        let js_iter =
            self.inner.resolve_starting_vertices(edge_name.as_ref(), parameters.into_js_dict());
        Box::new(JsVertexIterator::new(js_iter.into_iter()))
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'static>(
        &self,
        contexts: ContextIterator<'static, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'static, V, FieldValue> {
        let opaques: Box<dyn Iterator<Item = Opaque>> = Box::new(contexts.map(Opaque::new));

        let ctx_iter = JsContextIterator::new(opaques);
        let registry = ctx_iter.registry.clone();
        let js_iter =
            self.inner.resolve_property(ctx_iter, type_name.as_ref(), property_name.as_ref());
        Box::new(ContextAndValueIterator::new(js_iter, registry).map(|(opaque, value)| {
            // SAFETY: This `Opaque` was constructed just a few lines ago
            //         in this `resolve_property()` call, so the `V` type must be the same.
            let ctx = unsafe { opaque.into_inner() };

            (ctx, value)
        }))
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'static>(
        &self,
        contexts: ContextIterator<'static, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &CoreEdgeParameters,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'static, V, VertexIterator<'static, Self::Vertex>> {
        let opaques: Box<dyn Iterator<Item = Opaque>> = Box::new(contexts.map(Opaque::new));

        let ctx_iter = JsContextIterator::new(opaques);
        let registry = ctx_iter.registry.clone();
        let parameters: JsEdgeParameters = parameters.clone().into();

        let js_iter = self.inner.resolve_neighbors(
            ctx_iter,
            type_name.as_ref(),
            edge_name.as_ref(),
            parameters.into_js_dict(),
        );
        Box::new(ContextAndNeighborsIterator::new(js_iter, registry, self.constants.clone()).map(
            |(opaque, neighbors)| {
                // SAFETY: This `Opaque` was constructed just a few lines ago
                //         in this `resolve_neighbors()` call, so the `V` type must be the same.
                let ctx = unsafe { opaque.into_inner() };

                (ctx, neighbors)
            },
        ))
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'static>(
        &self,
        contexts: ContextIterator<'static, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'static, V, bool> {
        let opaques: Box<dyn Iterator<Item = Opaque>> = Box::new(contexts.map(Opaque::new));

        let ctx_iter = JsContextIterator::new(opaques);
        let registry = ctx_iter.registry.clone();
        let js_iter =
            self.inner.resolve_coercion(ctx_iter, type_name.as_ref(), coerce_to_type.as_ref());
        Box::new(ContextAndBoolIterator::new(js_iter, registry).map(|(opaque, value)| {
            // SAFETY: This `Opaque` was constructed just a few lines ago
            //         in this `resolve_coercion()` call, so the `V` type must be the same.
            let ctx = unsafe { opaque.into_inner() };

            (ctx, value)
        }))
    }
}
