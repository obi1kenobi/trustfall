import React, { useState, useEffect } from 'react';
import Editor from '@monaco-editor/react';
import {
    Box,
    GlobalStyles,
    FormControl,
    InputLabel,
    Select,
    MenuItem,
    SelectChangeEvent,
    Typography,
} from '@mui/material';

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
    const [exampleQueryId, setExampleQueryId] = useState('');

    const handleExampleQueryChange = (evt: SelectChangeEvent) => {
        setExampleQueryId(evt.currentTarget.value);
    };

    useEffect(() => {
        try {
            setSchema(wasm.infer_schema(query));
        } catch (e) {
            setSchema(e as string);
        }
    }, [query]);

    return (
        <>
            <GlobalStyles styles={{ body: { fontFamily: 'Roboto' } }} />
            <Box sx={{ display: 'flex' }}>
                <Box sx={{ flex: '1 0 auto' }}>
                    <Typography variant="h4" component="h1" sx={{ my: 2 }}>
                        Query
                        <FormControl sx={{ mx: 2, minWidth: 200 }} size="small">
                            <InputLabel id="example-query-select">Example Query</InputLabel>
                            <Select
                                labelId="example-query-select"
                                label="Example Query"
                                value={exampleQueryId}
                                onChange={handleExampleQueryChange}
                            >
                                <MenuItem value="">None</MenuItem>
                            </Select>
                        </FormControl>
                    </Typography>
                    <Box sx={{ border: '1px solid #eee' }}>
                        <Editor
                            defaultLanguage="graphql"
                            value={query}
                            onChange={setQuery}
                            height="500px"
                            options={{
                                minimap: {
                                    enabled: false,
                                },
                            }}
                        />
                    </Box>
                </Box>
                <Box sx={{ flex: '1 0 auto', marginLeft: 2 }}>
                    <Typography variant="h4" component="h1" sx={{ my: 2 }}>
                        Generated Schema
                    </Typography>
                    <Box sx={{ border: '1px solid #eee' }}>
                        <Editor
                            defaultLanguage="graphql"
                            value={schema}
                            height="500px"
                            options={{
                                minimap: {
                                    enabled: false,
                                },
                            }}
                            readOnly={false}
                        />
                    </Box>
                </Box>
            </Box>
        </>
    );
}
