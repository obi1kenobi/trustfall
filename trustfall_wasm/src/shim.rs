use std::{cell::RefCell, rc::Rc, collections::BTreeMap};

use serde::{Serialize, Deserialize};
use wasm_bindgen::prelude::*;

use trustfall_core::{interpreter::DataContext, ir::FieldValue};

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct JsContext {
    pub(super) local_id: u32,
    current_token: Option<JsValue>,
}

#[wasm_bindgen]
impl JsContext {
    pub(super) fn new(local_id: u32, current_token: Option<JsValue>) -> Self {
        Self { local_id, current_token }
    }

    #[wasm_bindgen(getter)]
    pub fn local_id(&self) -> u32 {
        self.local_id
    }

    #[wasm_bindgen(getter)]
    pub fn current_token(&self) -> JsValue {
        match &self.current_token {
            Some(value) => value.clone(),
            None => JsValue::NULL,
        }
    }
}

#[wasm_bindgen]
pub struct ContextIterator {
    iter: Box<dyn Iterator<Item = DataContext<JsValue>>>,
    pub(super) registry: Rc<RefCell<BTreeMap<u32, DataContext<JsValue>>>>,
    next_item: u32,
}

#[wasm_bindgen]
pub struct ContextIteratorItem {
    item: Option<JsContext>,
}

#[wasm_bindgen]
impl ContextIteratorItem {
    fn new_item(value: JsContext) -> Self {
        Self { item: Some(value) }
    }

    fn new_done() -> Self {
        Self { item: None }
    }

    #[wasm_bindgen(getter)]
    pub fn done(&self) -> bool {
        self.item.is_none()
    }

    #[wasm_bindgen(getter)]
    pub fn value(&self) -> Option<JsContext> {
        self.item.clone()
    }
}

#[wasm_bindgen]
impl ContextIterator {
    pub(super) fn new(iter: Box<dyn Iterator<Item = DataContext<JsValue>>>) -> Self {
        Self {
            iter,
            registry: Rc::from(RefCell::new(Default::default())),
            next_item: 0,
        }
    }

    #[wasm_bindgen(js_name = "next")]
    pub fn advance(&mut self) -> ContextIteratorItem {
        let next = self.iter.next();
        if let Some(ctx) = next {
            let next_item = self.next_item;
            self.next_item = self.next_item.wrapping_add(1);
            let current_token = ctx.current_token.clone();

            let existing = self.registry.borrow_mut().insert(next_item, ctx);
            assert!(existing.is_none(), "id {} already inserted with value {:?}", next_item, existing);

            ContextIteratorItem::new_item(JsContext::new(next_item, current_token))
        } else {
            ContextIteratorItem::new_done()
        }
    }
}

#[wasm_bindgen]
pub struct EdgeParameters {}  // TODO

/// The (context, value) iterator item returned by the WASM version
/// of the project_property() adapter method.
#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnedContextIdAndValue {
    pub(super) local_id: u32,
    pub(super) value: ReturnedValue,
}

impl ReturnedContextIdAndValue {
    pub fn local_id(&self) -> u32 {
        self.local_id
    }

    pub fn value(&self) -> &ReturnedValue {
        &self.value
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReturnedValue {
    Null,
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    List(Vec<ReturnedValue>),
}

impl From<ReturnedValue> for FieldValue {
    fn from(v: ReturnedValue) -> Self {
        match v {
            ReturnedValue::Null => FieldValue::Null,
            ReturnedValue::String(s) => FieldValue::String(s),
            ReturnedValue::Integer(i) => FieldValue::Int64(i),
            ReturnedValue::Float(n) => FieldValue::Float64(n),
            ReturnedValue::Boolean(b) => FieldValue::Boolean(b),
            ReturnedValue::List(v) => FieldValue::List(v.into_iter().map(|x| x.into()).collect()),
        }
    }
}

/// The (context, can_coerce) iterator item returned by the WASM version
/// of the can_coerce_to_type() adapter method.
#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnedContextIdAndBool {
    pub local_id: u32,
    pub value: bool,
}
