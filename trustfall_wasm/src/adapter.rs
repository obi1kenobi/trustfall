use std::sync::Arc;

use trustfall_core::{ir::{EdgeParameters, Vid, FieldValue, Eid}, interpreter::{InterpretedQuery, DataContext, Adapter}};
use wasm_bindgen::prelude::*;


#[wasm_bindgen]
extern "C" {
    pub type JsAdapter;

    #[wasm_bindgen(structural, method)]
    pub fn get_starting_tokens(
        this: &JsAdapter,
        edge: &str,
    ) -> js_sys::Iterator;

    #[wasm_bindgen(structural, method)]
    pub fn project_property(
        this: &JsAdapter,
        data_contexts: js_sys::Iterator,  // TODO: this isn't the right kind of iterator
        current_type_name: &str,
        field_name: &str,
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
        let iter_next = self.inner.next().unwrap();

        if iter_next.done() {
            None
        } else {
            Some(iter_next.value())
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
        parameters: Option<Arc<EdgeParameters>>,
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
        todo!()
    }

    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'static>,
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
