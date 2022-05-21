import * as wasm from "schemaless_wasm";

wasm.init();

const result = wasm.send_iterator([1, 2, 3, 4, 5].values());
console.log("from JS: result=", result);

const iter = wasm.get_iterator();
const next = iter.next();

function make_iter(iter: wasm.MyIterator): IterableIterator<unknown> {
    return {
        "next": function () {
            const n = iter.next();
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

const wrapped = make_iter(wasm.get_iterator());
console.log("from JS: wrapped=", wrapped.next());
console.log("from JS: wrapped=", wrapped.next());
console.log("from JS: wrapped=", wrapped.next());

const arr = Array.from(make_iter(wasm.get_iterator()));
console.log("from JS: arr=", arr);

function queryChanged() {
    const query = document.querySelector("#query");
    const schema = document.querySelector("#schema");

    const query_text = query.value;
    try {
        const schema_text = wasm.infer_schema(query_text);
        schema.value = schema_text;
    } catch (e) {
        schema.value = e;
    }
}

document.querySelector("#query").oninput = queryChanged;
