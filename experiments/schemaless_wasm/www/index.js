import * as wasm from "schemaless_wasm";

wasm.init();

var result = wasm.send_iterator([1, 2, 3, 4, 5].values());
console.log("from JS: result=", result);

var iter = wasm.get_iterator();
var next = iter.next();

function make_iter(iter) {
    return {
        "next": function () {
            var n = iter.next();
            console.log("from make_iter(): n=", n);
            return {
                "done": n.done(),
                "value": n.value(),
            };
        },
        [Symbol.iterator]: function () { return this; }
    }
}

console.log("from JS: iter.next()=", next);

var wrapped = make_iter(wasm.get_iterator());
console.log("from JS: wrapped=", wrapped.next());
console.log("from JS: wrapped=", wrapped.next());
console.log("from JS: wrapped=", wrapped.next());

var arr = Array.from(make_iter(wasm.get_iterator()));
console.log("from JS: arr=", arr);

function queryChanged() {
    var query = document.querySelector("#query");
    var schema = document.querySelector("#schema");

    var query_text = query.value;
    try {
        var schema_text = wasm.infer_schema(query_text);
        schema.value = schema_text;
    } catch (e) {
        schema.value = e;
    }
}

document.querySelector("#query").oninput = queryChanged;
