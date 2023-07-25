use schemaless::infer_schema_from_query;
use wasm_bindgen::prelude::*;

use crate::util::set_panic_hook;

#[macro_use]
mod util;

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
}

// Needs `export NODE_OPTIONS=--openssl-legacy-provider` to start correctly under Node 17.

#[wasm_bindgen]
pub fn init() {
    set_panic_hook();
}

#[wasm_bindgen]
pub fn infer_schema(query: &str) -> Result<String, String> {
    infer_schema_from_query(query)
}

// to improve this, use this:
// https://rustwasm.github.io/wasm-bindgen/reference/iterating-over-js-values.html
#[wasm_bindgen]
pub fn send_iterator(iter: js_sys::Iterator) -> Vec<f64> {
    let mut result = Vec::new();

    loop {
        let next = match iter.next() {
            Ok(iter_next) => {
                if iter_next.done() {
                    break;
                } else {
                    iter_next.value()
                }
            }
            Err(val) => {
                panic!("iterator returned Err: {val:?}");
            }
        };

        result.push(next.as_f64().expect("did not get a number"));
    }

    result
}

#[wasm_bindgen]
pub struct MyIteratorElement {
    done: bool,
    value: Option<i64>,
}

#[wasm_bindgen]
impl MyIteratorElement {
    pub fn done(&self) -> bool {
        self.done
    }

    pub fn value(&self) -> Option<i64> {
        self.value
    }
}

#[wasm_bindgen]
pub struct MyIterator {
    max_num: i64,
    current_num: i64,
}

#[wasm_bindgen]
#[allow(clippy::should_implement_trait)]
impl MyIterator {
    pub fn next(&mut self) -> MyIteratorElement {
        if self.current_num > self.max_num {
            MyIteratorElement { done: true, value: None }
        } else {
            let value = Some(self.current_num);
            self.current_num += 1;
            MyIteratorElement { done: false, value }
        }
    }
}

#[wasm_bindgen]
pub fn get_iterator() -> MyIterator {
    MyIterator { max_num: 5, current_num: 0 }
}
