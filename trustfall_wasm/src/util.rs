macro_rules! make_wasm_bindgen_struct_with_debug_clone {
    ($id:ident, $t:path) => {
        #[wasm_bindgen::prelude::wasm_bindgen]
        #[derive(Debug, Clone)]
        pub struct $id($t);

        impl $id {
            #[allow(dead_code)]
            fn new(inner: $t) -> Self {
                crate::set_panic_hook();
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
