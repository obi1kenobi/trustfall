import * as wasm from "trustfall_wasm";


function make_iter(iter) {
  return {
    "next": function () {
      var n = iter.next();
      return {
        "done": n.done,
        "value": n.value,
      };
    },
    [Symbol.iterator]: function () { return this; }
  }
}


class JsAdapter {
  /*
  #[wasm_bindgen(structural, method)]
  pub fn get_starting_tokens(this: &JsAdapter, edge: &str) -> js_sys::Iterator;
  */
  *get_starting_tokens(edge) {
    for (const num of [1, 2, 3, 4, 5, 6, 7, 8]) {
      yield num;
    }
  }

  /*
  #[wasm_bindgen(structural, method)]
  pub fn project_property(
    this: &JsAdapter,
    data_contexts: ContextIterator,
    current_type_name: &str,
    field_name: &str,
  ) -> js_sys::Iterator;
  */
  *project_property(data_contexts, current_type_name, field_name) {
    const ctxs = make_iter(data_contexts);
    console.log("ctxs=", ctxs);
    if (current_type_name === "Number" || current_type_name === "Prime" || current_type_name === "Composite") {
      if (field_name === "value") {
        for (const ctx of ctxs) {
          const val = {
            local_id: ctx.local_id,
            value: ctx.current_token,
          };
          console.log("proj_prop yielding=", val);
          console.log("converts to=", JSON.stringify(val));
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
  #[wasm_bindgen(structural, method)]
  pub fn project_neighbors(
    this: &JsAdapter,
    data_contexts: ContextIterator,
    current_type_name: &str,
    edge_name: &str,
    parameters: Option<EdgeParameters>,
  ) -> js_sys::Iterator;
  */
  *project_neighbors(data_contexts, current_type_name, edge_name, parameters) {
    const ctxs = make_iter(data_contexts);
    if (current_type_name === "Number" || current_type_name === "Prime" || current_type_name === "Composite") {
      if (edge_name === "successor") {
        for (const ctx of ctxs) {
          const val = [
            ctx.local_id,
            [ctx.current_token + 1],
          ];
          console.log("proj_nbrs yielding=", val);
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
  #[wasm_bindgen(structural, method)]
  pub fn can_coerce_to_type(
    this: &JsAdapter,
    data_contexts: ContextIterator,
    current_type_name: &str,
    coerce_to_type_name: &str,
  ) -> js_sys::Iterator;
  */
  *can_coerce_to_type(data_contexts, current_type_name, coerce_to_type_name) {
    const ctxs = make_iter(data_contexts);
    const primes = {
      2: null,
      3: null,
      5: null,
      7: null,
      11: null,
    };
    console.log("ctxs=", ctxs);
    if (current_type_name === "Number") {
      if (coerce_to_type_name === "Prime") {
        for (const ctx of ctxs) {
          var can_coerce = false;
          if (ctx.current_token in primes) {
            can_coerce = true;
          }
          const val = {
            local_id: ctx.local_id,
            value: can_coerce,
          };
          console.log(`can_coerce ${ctx.current_token} yielding=`, val);
          console.log("converts to=", JSON.stringify(val));
          yield val;
        }
      } else if (coerce_to_type_name === "Composite") {
        for (const ctx of ctxs) {
          var can_coerce = false;
          if (!(ctx.current_token in primes || ctx.current_token === 1)) {
            can_coerce = true;
          }
          const val = {
            local_id: ctx.local_id,
            value: can_coerce,
          };
          console.log(`can_coerce ${ctx.current_token} yielding=`, val);
          console.log("converts to=", JSON.stringify(val));
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

try {
  wasm.attempt(
    new JsAdapter(),
    `{
      Number(max: 10) {
        ... on Prime {
          value @output

          successor {
            next: value @output
          }
        }
      }
    }`,
  );
  console.log("success!");
} catch (e) {
  console.error("failure: ", e);
}
