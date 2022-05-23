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
import { EXAMPLE_QUERY_MAP } from './exampleQueries';

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

const queryOptions: Array<keyof typeof EXAMPLE_QUERY_MAP> = [
    'actions_in_repos_with_min_hn_pts',
    'crates_io_github_actions',
    'hackernews_patio11_own_post_comments',
    'repos_with_min_hackernews_points',
];

export default function App(): JSX.Element {
    const [query, setQuery] = useState<string | undefined>('');
    const [schema, setSchema] = useState('');
    const [exampleQueryId, setExampleQueryId] = useState<
        keyof typeof EXAMPLE_QUERY_MAP | undefined
    >(undefined);

    const handleExampleQueryChange = (evt: SelectChangeEvent) => {
        const { value } = evt.target;
        if (value in EXAMPLE_QUERY_MAP) {
            setExampleQueryId(value as keyof typeof EXAMPLE_QUERY_MAP);
        }
    };

    useEffect(() => {
        if (!query || query === '') {
            setSchema('# Enter a query on the left, or select an example query from the dropdown.');
        } else {
            try {
                setSchema(wasm.infer_schema(query));
            } catch (e) {
                const message = `# An error was encountered:\n${e}`;
                setSchema(message);
            }
        }
    }, [query]);

    useEffect(() => {
        if (exampleQueryId) {
            setQuery(EXAMPLE_QUERY_MAP[exampleQueryId].query);
        } else {
            setQuery('');
        }
    }, [exampleQueryId]);

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
                                {queryOptions.map((key) => (
                                    <MenuItem key={key} value={key}>
                                        {EXAMPLE_QUERY_MAP[key].label}
                                    </MenuItem>
                                ))}
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
                                readOnly: true,
                                minimap: {
                                    enabled: false,
                                },
                            }}
                        />
                    </Box>
                </Box>
            </Box>
        </>
    );
}
