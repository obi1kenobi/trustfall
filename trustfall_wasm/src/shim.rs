use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use trustfall_core::{
    interpreter::{DataContext, VertexIterator},
    ir::FieldValue,
};

#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsFieldValue {
    Null,
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    List(Vec<JsFieldValue>),
}

impl From<JsFieldValue> for FieldValue {
    fn from(v: JsFieldValue) -> Self {
        match v {
            JsFieldValue::Null => FieldValue::Null,
            JsFieldValue::String(s) => FieldValue::String(s),
            JsFieldValue::Integer(i) => FieldValue::Int64(i),
            JsFieldValue::Float(n) => FieldValue::Float64(n),
            JsFieldValue::Boolean(b) => FieldValue::Boolean(b),
            JsFieldValue::List(v) => FieldValue::List(v.into_iter().map(|x| x.into()).collect()),
        }
    }
}

impl From<FieldValue> for JsFieldValue {
    fn from(v: FieldValue) -> Self {
        match v {
            FieldValue::Null => JsFieldValue::Null,
            FieldValue::String(s) => JsFieldValue::String(s),
            FieldValue::Int64(i) => JsFieldValue::Integer(i),
            FieldValue::Uint64(u) => match i64::try_from(u) {
                Ok(i) => JsFieldValue::Integer(i),
                Err(_) => JsFieldValue::Float(u as f64),
            },
            FieldValue::Float64(n) => JsFieldValue::Float(n),
            FieldValue::Boolean(b) => JsFieldValue::Boolean(b),
            FieldValue::List(v) => JsFieldValue::List(v.into_iter().map(|x| x.into()).collect()),
            FieldValue::DateTimeUtc(_) => unimplemented!(),
            FieldValue::Enum(_) => unimplemented!(),
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JsEdgeParameters {
    values: BTreeMap<String, JsFieldValue>,
}

#[wasm_bindgen]
impl JsEdgeParameters {
    pub fn get(&self, name: &str) -> JsValue {
        let value = self
            .values
            .get(name)
            .expect("no edge parameter by that name");

        JsValue::from_serde(&value).expect("serde conversion failed")
    }

    pub fn into_js_dict(&self) -> JsValue {
        JsValue::from_serde(&self.values).expect("serde conversion failed")
    }
}

impl From<trustfall_core::ir::EdgeParameters> for JsEdgeParameters {
    fn from(p: trustfall_core::ir::EdgeParameters) -> Self {
        Self::from(&p)
    }
}

impl From<&trustfall_core::ir::EdgeParameters> for JsEdgeParameters {
    fn from(p: &trustfall_core::ir::EdgeParameters) -> Self {
        Self {
            values: p
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone().into()))
                .collect(),
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct JsContext {
    #[wasm_bindgen(js_name = "localId")]
    pub local_id: u32,
    current_token: Option<JsValue>,
}

#[wasm_bindgen]
impl JsContext {
    pub(super) fn new(local_id: u32, current_token: Option<JsValue>) -> Self {
        Self {
            local_id,
            current_token,
        }
    }

    #[wasm_bindgen(getter, js_name = "currentToken")]
    pub fn current_token(&self) -> JsValue {
        match &self.current_token {
            Some(value) => value.clone(),
            None => JsValue::NULL,
        }
    }
}

pub(super) struct JsStringConstants {
    pub(super) local_id: JsValue,
    pub(super) neighbors: JsValue,
}

impl JsStringConstants {
    pub(super) fn new() -> Self {
        Self {
            local_id: JsValue::from_str("localId"),
            neighbors: JsValue::from_str("neighbors"),
        }
    }
}

#[wasm_bindgen]
pub struct JsContextIterator {
    iter: VertexIterator<'static, DataContext<JsValue>>,
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
impl JsContextIterator {
    pub(super) fn new(iter: VertexIterator<'static, DataContext<JsValue>>) -> Self {
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
            assert!(
                existing.is_none(),
                "id {next_item} already inserted with value {existing:?}",
            );

            ContextIteratorItem::new_item(JsContext::new(next_item, current_token))
        } else {
            ContextIteratorItem::new_done()
        }
    }
}

/// The (context, value) iterator item returned by the WASM version
/// of the resolve_property() adapter method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnedContextIdAndValue {
    #[serde(rename = "localId")]
    pub local_id: u32,
    pub(super) value: JsFieldValue,
}

/// The (context, can_coerce) iterator item returned by the WASM version
/// of the resolve_coercion() adapter method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnedContextIdAndBool {
    #[serde(rename = "localId")]
    pub local_id: u32,
    pub value: bool,
}

#[wasm_bindgen]
pub struct QueryResultIterator {
    iter: Box<dyn Iterator<Item = BTreeMap<Arc<str>, FieldValue>>>,
}

#[wasm_bindgen]
pub struct QueryResultItem {
    item: Option<BTreeMap<Arc<str>, JsFieldValue>>,
}

#[wasm_bindgen]
impl QueryResultItem {
    fn new_item(value: BTreeMap<Arc<str>, JsFieldValue>) -> Self {
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
    pub fn value(&self) -> JsValue {
        JsValue::from_serde(&self.item).expect("serde conversion failed")
    }
}

impl QueryResultIterator {
    pub fn new(iter: Box<dyn Iterator<Item = BTreeMap<Arc<str>, FieldValue>>>) -> Self {
        Self { iter }
    }
}

#[wasm_bindgen]
impl QueryResultIterator {
    #[wasm_bindgen(js_name = "next")]
    pub fn advance(&mut self) -> QueryResultItem {
        let next = self.iter.next();
        if let Some(result) = next {
            QueryResultItem::new_item(result.into_iter().map(|(k, v)| (k, v.into())).collect())
        } else {
            QueryResultItem::new_done()
        }
    }
}
