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
    if (current_type_name === "Number") {
      if (field_name === "value") {
        for (const ctx of ctxs) {
          const val = {
            local_id: ctx.local_id,
            value: ctx.current_token,
          };
          console.log("yielding=", val);
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

  }
}

try {
  wasm.attempt(
    new JsAdapter(),
    `{
      Number(max: 10) {
        value @output
      }
    }`,
  );
  console.log("success!");
} catch (e) {
  console.error("failure: ", e);
}
