use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

use trustfall_wasm::{
    adapter::{AdapterShim, JsAdapter},
    shim::JsFieldValue,
    Schema,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
    class JsNumbersAdapter {
        /*
        #[wasm_bindgen(structural, method, js_name = "getStartingTokens")]
        pub fn get_starting_tokens(this: &JsAdapter, edge: &str) -> js_sys::Iterator;
        */
        *getStartingTokens(edge, parameters) {
            if (edge === "Number") {
                const params = parameters.into_js_dict();
                const maxValue = params["max"];
                for (var i = 1; i <= maxValue; i++) {
                    yield i;
                }
            } else {
                throw `unreachable edge name: ${edge}`;
            }
        }

        /*
        #[wasm_bindgen(structural, method, js_name = "projectProperty")]
        pub fn project_property(
            this: &JsAdapter,
            data_contexts: ContextIterator,
            current_type_name: &str,
            field_name: &str,
        ) -> js_sys::Iterator;
        */
        *projectProperty(data_contexts, current_type_name, field_name) {
            if (current_type_name === "Number" || current_type_name === "Prime" || current_type_name === "Composite") {
                if (field_name === "value") {
                    for (const ctx of data_contexts) {
                        const val = {
                            localId: ctx.localId,
                            value: ctx.currentToken,
                        };
                        yield val;
                    }
                } else {
                    throw `unreachable field name: ${current_type_name} ${field_name}`;
                }
            } else {
                throw `unreachable type name: ${current_type_name} ${field_name}`;
            }
        }

        /*
        #[wasm_bindgen(structural, method, js_name = "projectNeighbors")]
        pub fn project_neighbors(
            this: &JsAdapter,
            data_contexts: ContextIterator,
            current_type_name: &str,
            edge_name: &str,
            parameters: Option<EdgeParameters>,
        ) -> js_sys::Iterator;
        */
        *projectNeighbors(data_contexts, current_type_name, edge_name, parameters) {
            if (current_type_name === "Number" || current_type_name === "Prime" || current_type_name === "Composite") {
                if (edge_name === "successor") {
                    for (const ctx of data_contexts) {
                        const val = {
                            localId: ctx.localId,
                            neighbors: [ctx.currentToken + 1],
                        };
                        yield val;
                    }
                } else {
                    throw `unreachable neighbor name: ${current_type_name} ${field_name}`;
                }
            } else {
                throw `unreachable type name: ${current_type_name} ${field_name}`;
            }
        }

        /*
        #[wasm_bindgen(structural, method, js_name = "canCoerceToType")]
        pub fn can_coerce_to_type(
            this: &JsAdapter,
            data_contexts: ContextIterator,
            current_type_name: &str,
            coerce_to_type_name: &str,
        ) -> js_sys::Iterator;
        */
        *canCoerceToType(data_contexts, current_type_name, coerce_to_type_name) {
            const primes = {
                2: null,
                3: null,
                5: null,
                7: null,
                11: null,
            };
            if (current_type_name === "Number") {
                if (coerce_to_type_name === "Prime") {
                    for (const ctx of data_contexts) {
                        var can_coerce = false;
                        if (ctx.currentToken in primes) {
                            can_coerce = true;
                        }
                        const val = {
                            localId: ctx.localId,
                            value: can_coerce,
                        };
                        yield val;
                    }
                } else if (coerce_to_type_name === "Composite") {
                    for (const ctx of data_contexts) {
                        var can_coerce = false;
                        if (!(ctx.currentToken in primes || ctx.currentToken === 1)) {
                            can_coerce = true;
                        }
                        const val = {
                            localId: ctx.localId,
                            value: can_coerce,
                        };
                        yield val;
                    }
                } else {
                    throw `unreachable coercion type name: ${current_type_name} ${coerce_to_type_name}`;
                }
            } else {
                throw `unreachable type name: ${current_type_name} ${coerce_to_type_name}`;
            }
        }
    }

    export function make_adapter() {
        return new JsNumbersAdapter();
    }
"#)]
extern "C" {
    pub fn make_adapter() -> JsAdapter;
}

pub fn run_numbers_query(
    query: &str,
    args: BTreeMap<String, JsFieldValue>,
) -> Result<Vec<BTreeMap<String, JsFieldValue>>, String> {
    trustfall_wasm::util::init().expect("init failed");

    let schema = trustfall_core::schema::Schema::parse(include_str!(
        "../../trustfall_core/src/resources/schemas/numbers.graphql"
    ))
    .unwrap();
    let adapter = make_adapter();

    let query = trustfall_core::frontend::parse(&schema, query).map_err(|e| e.to_string())?;

    let wrapped_adapter = Rc::new(RefCell::new(AdapterShim::new(adapter)));

    let results: Vec<_> = trustfall_core::interpreter::execution::interpret_ir(
        wrapped_adapter,
        query,
        Arc::new(
            args.into_iter()
                .map(|(k, v)| (Arc::from(k), v.into()))
                .collect(),
        ),
    )
    .map_err(|e| e.to_string())?
    .map(|res| {
        res.into_iter()
            .map(|(k, v)| (k.to_string(), v.into()))
            .collect()
    })
    .collect();

    Ok(results)
}

pub fn make_test_schema() -> Schema {
    let schema_text = "\
schema {
    query: RootSchemaQuery
}
directive @filter(op: String!, value: [String!]) on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD

type RootSchemaQuery {
    Number(max: Int!): [Number!]
}

type Number {
    name: String
    value: Int!

    predecessor: Number
    successor: Number!
    multiple(max: Int!): [Number!]
}";

    Schema::parse(schema_text).unwrap()
}
