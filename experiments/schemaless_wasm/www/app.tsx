import React, { useState, useEffect } from 'react';
import * as wasm from 'schemaless_wasm';

wasm.init();

const result = wasm.send_iterator([1, 2, 3, 4, 5].values());
console.log('from JS: result=', result);

const iter = wasm.get_iterator();
const next = iter.next();

function make_iter(iter: wasm.MyIterator): IterableIterator<unknown> {
    return {
        next: function () {
            const n = iter.next();
            console.log('from make_iter(): n=', n);
            return {
                done: n.done(),
                value: n.value(),
            };
        },
        [Symbol.iterator]: function () {
            return this;
        },
    };
}

console.log('from JS: iter.next()=', next);

const wrapped = make_iter(wasm.get_iterator());
console.log('from JS: wrapped=', wrapped.next());
console.log('from JS: wrapped=', wrapped.next());
console.log('from JS: wrapped=', wrapped.next());

const arr = Array.from(make_iter(wasm.get_iterator()));
console.log('from JS: arr=', arr);

export default function App(): JSX.Element {
    const [query, setQuery] = useState('');
    const [schema, setSchema] = useState('');

    const handleQueryChange = (evt: React.ChangeEvent<HTMLTextAreaElement>) => {
        console.log("Setting query")
        setQuery(evt.currentTarget.value);
    };

    useEffect(() => {
        try {
            setSchema(wasm.infer_schema(query));
        } catch (e) {
            setSchema(e as string);
        }
    }, [query])

    return (
        <main>
            <textarea
                value={query}
                onChange={handleQueryChange}
                rows={40}
                cols={80}
                placeholder="Enter your query here..."
                autoFocus
            />
            <textarea
                value={schema}
                rows={40}
                cols={80}
                placeholder="Your generated schema will appear here..."
                readOnly
            />
        </main>
    );
}
