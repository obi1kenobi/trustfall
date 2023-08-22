use std::{collections::BTreeMap, sync::Arc};

use trustfall_wasm::{
    adapter::{AdapterShim, JsAdapter},
    shim::JsFieldValue,
    Schema,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
    class JsNumbersAdapter {
        /*
        #[wasm_bindgen(structural, method, js_name = "resolveStartingVertices")]
        pub fn resolve_starting_vertices(this: &JsAdapter, edge: &str) -> js_sys::Iterator;
        */
        *resolveStartingVertices(edge, parameters) {
            if (edge === "Number") {
                const maxValue = parameters["max"];
                for (var i = 1; i <= maxValue; i++) {
                    yield i;
                }
            } else {
                throw `unreachable edge name: ${edge}`;
            }
        }

        /*
        #[wasm_bindgen(structural, method, js_name = "resolveProperty")]
        pub fn resolve_property(
            this: &JsAdapter,
            contexts: ContextIterator,
            type_name: &str,
            field_name: &str,
        ) -> js_sys::Iterator;
        */
        *resolveProperty(contexts, type_name, field_name) {
            if (type_name === "Number" || type_name === "Prime" || type_name === "Composite") {
                if (field_name === "value") {
                    for (const ctx of contexts) {
                        const val = {
                            localId: ctx.localId,
                            value: ctx.activeVertex,
                        };
                        yield val;
                    }
                } else {
                    throw `unreachable field name: ${type_name} ${field_name}`;
                }
            } else {
                throw `unreachable type name: ${type_name} ${field_name}`;
            }
        }

        /*
        #[wasm_bindgen(structural, method, js_name = "resolveNeighbors")]
        pub fn resolve_neighbors(
            this: &JsAdapter,
            contexts: ContextIterator,
            type_name: &str,
            edge_name: &str,
            parameters: Option<EdgeParameters>,
        ) -> js_sys::Iterator;
        */
        *resolveNeighbors(contexts, type_name, edge_name, parameters) {
            if (type_name === "Number" || type_name === "Prime" || type_name === "Composite") {
                if (edge_name === "successor") {
                    for (const ctx of contexts) {
                        const val = {
                            localId: ctx.localId,
                            neighbors: [ctx.activeVertex + 1],
                        };
                        yield val;
                    }
                } else {
                    throw `unreachable neighbor name: ${type_name} ${field_name}`;
                }
            } else {
                throw `unreachable type name: ${type_name} ${field_name}`;
            }
        }

        /*
        #[wasm_bindgen(structural, method, js_name = "resolveCoercion")]
        pub fn resolve_coercion(
            this: &JsAdapter,
            contexts: ContextIterator,
            type_name: &str,
            coerce_to_type: &str,
        ) -> js_sys::Iterator;
        */
        *resolveCoercion(contexts, type_name, coerce_to_type) {
            const primes = {
                2: null,
                3: null,
                5: null,
                7: null,
                11: null,
            };
            if (type_name === "Number") {
                if (coerce_to_type === "Prime") {
                    for (const ctx of contexts) {
                        var can_coerce = false;
                        if (ctx.activeVertex in primes) {
                            can_coerce = true;
                        }
                        const val = {
                            localId: ctx.localId,
                            value: can_coerce,
                        };
                        yield val;
                    }
                } else if (coerce_to_type === "Composite") {
                    for (const ctx of contexts) {
                        var can_coerce = false;
                        if (!(ctx.activeVertex in primes || ctx.activeVertex === 1)) {
                            can_coerce = true;
                        }
                        const val = {
                            localId: ctx.localId,
                            value: can_coerce,
                        };
                        yield val;
                    }
                } else {
                    throw `unreachable coercion type name: ${type_name} ${coerce_to_type}`;
                }
            } else {
                throw `unreachable type name: ${type_name} ${coerce_to_type}`;
            }
        }
    }

    export function makeAdapter() {
        return new JsNumbersAdapter();
    }
"#)]
extern "C" {
    #[wasm_bindgen(js_name = "makeAdapter")]
    pub fn make_adapter() -> JsAdapter;
}

pub fn run_numbers_query(
    query: &str,
    args: BTreeMap<String, JsFieldValue>,
) -> Result<Vec<BTreeMap<String, JsFieldValue>>, String> {
    trustfall_wasm::util::initialize().expect("init failed");

    let schema = trustfall_core::schema::Schema::parse(include_str!(
        "../../trustfall_core/test_data/schemas/numbers.graphql"
    ))
    .unwrap();
    let adapter = make_adapter();

    let query = trustfall_core::frontend::parse(&schema, query).map_err(|e| e.to_string())?;

    let wrapped_adapter = Arc::new(AdapterShim::new(adapter));

    let results: Vec<_> = trustfall_core::interpreter::execution::interpret_ir(
        wrapped_adapter,
        query,
        Arc::new(args.into_iter().map(|(k, v)| (Arc::from(k), v.into())).collect()),
    )
    .map_err(|e| e.to_string())?
    .map(|res| res.into_iter().map(|(k, v)| (k.to_string(), v.into())).collect())
    .collect();

    Ok(results)
}

pub fn make_test_schema() -> Schema {
    let schema_text = "\
schema {
    query: RootSchemaQuery
}
directive @filter(op: String!, value: [String!]) repeatable on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD
directive @transform(op: String!) on FIELD

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
