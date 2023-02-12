import { Schema, executeQuery, initialize } from "trustfall_wasm";

const numbersSchema = Schema.parse(`
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
  #[wasm_bindgen(structural, method, js_name = "getStartingTokens")]
  pub fn resolve_starting_vertices(this: &JsAdapter, edge: &str) -> js_sys::Iterator;
  */
  *getStartingTokens(edge, parameters) {
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
  #[wasm_bindgen(structural, method, js_name = "projectProperty")]
  pub fn resolve_property(
      this: &JsAdapter,
      data_contexts: ContextIterator,
      type_name: &str,
      field_name: &str,
  ) -> js_sys::Iterator;
  */
  *projectProperty(data_contexts, type_name, field_name) {
    if (type_name === "Number" || type_name === "Prime" || type_name === "Composite") {
      if (field_name === "value") {
        for (const ctx of data_contexts) {
          const val = {
            localId: ctx.localId,
            value: ctx.currentToken,
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
  #[wasm_bindgen(structural, method, js_name = "projectNeighbors")]
  pub fn resolve_neighbors(
      this: &JsAdapter,
      data_contexts: ContextIterator,
      type_name: &str,
      edge_name: &str,
      parameters: Option<EdgeParameters>,
  ) -> js_sys::Iterator;
  */
  *projectNeighbors(data_contexts, type_name, edge_name, parameters) {
    if (type_name === "Number" || type_name === "Prime" || type_name === "Composite") {
      if (edge_name === "successor") {
        for (const ctx of data_contexts) {
          const val = {
            localId: ctx.localId,
            neighbors: [ctx.currentToken + 1],
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
  #[wasm_bindgen(structural, method, js_name = "canCoerceToType")]
  pub fn resolve_coercion(
      this: &JsAdapter,
      data_contexts: ContextIterator,
      type_name: &str,
      coerce_to_type_name: &str,
  ) -> js_sys::Iterator;
  */
  *canCoerceToType(data_contexts, type_name, coerce_to_type_name) {
    const primes = {
      2: null,
      3: null,
      5: null,
      7: null,
      11: null,
    };
    if (type_name === "Number") {
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
        throw `unreachable coercion type name: ${type_name} ${coerce_to_type_name}`;
      }
    } else {
      throw `unreachable type name: ${type_name} ${coerce_to_type_name}`;
    }
  }
}

function runQuery() {
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

  for (const result of executeQuery(numbersSchema, adapter, query, args)) {
    console.log("result=", result);
  }
}

initialize();
runQuery();
