use common::{make_test_schema, run_numbers_query};
use trustfall_core::ir::FieldValue;
use trustfall_wasm::shim::JsFieldValue;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[macro_use]
extern crate maplit;

mod common;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen(start)]
pub fn run_at_start() {
    trustfall_wasm::util::initialize().expect("initialize failed");
}

#[wasm_bindgen_test]
pub fn test_schema() {
    make_test_schema();
}

#[wasm_bindgen_test]
pub fn deserialize_returned_value() {
    let value: JsFieldValue = serde_json::from_str("1").expect("could not deserialize");
    let field_value: FieldValue = value.into();
    assert_eq!(field_value, FieldValue::Int64(1));
}

#[wasm_bindgen(inline_js = r#"
import {Schema, executeQuery} from "../../wasm-bindgen-test";

export function testQuery() {
    const numbersSchema = Schema.parse(`
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
    Number(min: Int = 0, max: Int!): [Number!]
    Zero: Number!
    One: Number!
    Two: Prime!
    Four: Composite!
}

interface Named {
    name: String
}

interface Number implements Named {
    name: String
    value: Int
    vowelsInName: [String]

    predecessor: Number
    successor: Number!
    multiple(max: Int!): [Composite!]
}

type Prime implements Number & Named {
    name: String
    value: Int
    vowelsInName: [String]

    predecessor: Number
    successor: Number!
    multiple(max: Int!): [Composite!]
}

type Composite implements Number & Named {
    name: String
    value: Int
    vowelsInName: [String]

    predecessor: Number
    successor: Number!
    multiple(max: Int!): [Composite!]
    divisor: [Number!]!
    primeFactor: [Prime!]!
}

type Letter implements Named {
    name: String
}
`);

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

    var adapter = new JsNumbersAdapter();

    const query = `
{
    Number(max: 10) {
        ... on Prime {
            value @output @filter(op: ">", value: ["$val"])

            successor {
                next: value @output
            }
        }
    }
}`;
    const args = {
        "val": 2,
    };

    const results = Array.from(executeQuery(numbersSchema, adapter, query, args));
    const expected_results = [
        {
            "next": 4,
            "value": 3,
        }, {
            "next": 6,
            "value": 5,
        }, {
            "next": 8,
            "value": 7,
        },
    ];

    // TODO: Is there a better way to compare arrays of objects without a ton of extra code?
    if (JSON.stringify(results) !== JSON.stringify(expected_results)) {
        console.log("received: ", JSON.stringify(results));
        throw "mismatch!";
    }
}
"#)]
extern "C" {
    // TODO: reuse existing adapter code instead of copy-pasting it here.
    #[wasm_bindgen(js_name = "testQuery")]
    pub fn js_test_query();
}

#[wasm_bindgen_test]
pub fn test_query() {
    js_test_query();
}

#[wasm_bindgen_test]
pub fn test_execute_query_with_traversal_and_coercion() {
    let query = r#"
{
    Number(max: 10) {
        ... on Prime {
            value @output

            successor {
                next: value @output
            }
        }
    }
}"#;
    let args = Default::default();

    let actual_results = run_numbers_query(query, args).expect("query and args were not valid");

    let expected_results = [
        btreemap! {
            String::from("value") => JsFieldValue::Integer(2),
            String::from("next") => JsFieldValue::Integer(3),
        },
        btreemap! {
            String::from("value") => JsFieldValue::Integer(3),
            String::from("next") => JsFieldValue::Integer(4),
        },
        btreemap! {
            String::from("value") => JsFieldValue::Integer(5),
            String::from("next") => JsFieldValue::Integer(6),
        },
        btreemap! {
            String::from("value") => JsFieldValue::Integer(7),
            String::from("next") => JsFieldValue::Integer(8),
        },
    ];

    assert_eq!(expected_results.as_slice(), actual_results);
}
