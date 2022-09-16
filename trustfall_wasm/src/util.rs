use js_sys::Object;
use wasm_bindgen::prelude::*;

use crate::shim::{ContextIterator, QueryResultIterator};

pub fn set_panic_hook() {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

macro_rules! make_wasm_bindgen_struct_with_debug_clone {
    ($id:ident, $t:path) => {
        #[wasm_bindgen::prelude::wasm_bindgen(inspectable)]
        #[derive(Debug, Clone)]
        pub struct $id($t);

        impl $id {
            #[allow(dead_code)]
            fn new(inner: $t) -> Self {
                Self(inner)
            }
        }

        impl std::ops::Deref for $id {
            type Target = $t;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
}

// A macro to provide `println!(..)`-style syntax for `console.log` logging.
#[allow(unused_macros)]
macro_rules! log {
    ( $( $t:tt )* ) => {
        web_sys::console::log_1(&format!( $( $t )* ).into());
    }
}

#[wasm_bindgen(inline_js = "
    function iterify(obj) {
        obj[Symbol.iterator] = function () {
            return this;
        };
    }
    module.exports.iterify = iterify;
")]
extern "C" {
    pub fn iterify(obj: &Object);
}

#[wasm_bindgen]
pub fn initialize() -> Result<(), JsValue> {
    set_panic_hook();

    // Update the ContextIterator and QueryResultIterator prototypes to make them be iterators.
    // This uses the workaround suggested in https://github.com/rustwasm/wasm-bindgen/issues/1478
    //
    // One day, it might not be required to instantiate an object and patch its prototype
    // through Javascript. That will be a day to celebrate.
    let x = ContextIterator::new(Box::new(std::iter::empty()));
    iterify(&Object::get_prototype_of(&x.into()));

    let x: QueryResultIterator = QueryResultIterator::new(Box::new(std::iter::empty()));
    iterify(&Object::get_prototype_of(&x.into()));

    Ok(())
}
