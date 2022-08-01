import { useCallback, useState, useEffect, useRef } from 'react';
import { buildSchema } from 'graphql';
import * as monaco from 'monaco-editor/esm/vs/editor/editor.api';
import { initializeMode } from 'monaco-graphql/esm/initializeMode';
import { css } from '@emotion/react';
import {
  Button,
  Grid,
  Paper,
  FormControl,
  InputLabel,
  Select,
  SelectChangeEvent,
  MenuItem,
  Typography,
  SxProps,
} from '@mui/material';
import { LoadingButton } from '@mui/lab';

import { HN_SCHEMA } from './adapter';
import { AsyncValue } from './types';
import parseExample from './utils/parseExample';
import latestStoriesExample from '../example_queries/latest_stories_with_min_points_and_submitter_karma.example';
import patio11Example from '../example_queries/patio11_commenting_on_submissions_of_his_blog_posts.example';
import topStoriesExample from '../example_queries/top_stories_with_min_points_and_submitter_karma.example';

// Position absolute is necessary to keep the editor from growing constantly on window resize
// This is due to the height: 100% rule, since the container is slightly smaller
const cssEditor = css`
  position: absolute;
  top: 0;
  width: calc(100% - 5px);
  height: calc(100% - 3px);
`;

const sxEditorContainer: SxProps = {
  border: "1px solid #aaa",
  borderRadius: "5px",
  padding: "3px",
}

// Disable editor gutters entirely
const disableGutterConfig: monaco.editor.IStandaloneEditorConstructionOptions = {
  lineNumbers: 'off',
  glyphMargin: false,
  folding: false,
  // Undocumented see https://github.com/Microsoft/vscode/issues/30795#issuecomment-410998882
  lineDecorationsWidth: 5,  // Leave a little padding on the left
  lineNumbersMinChars: 0
}

initializeMode({
  schemas: [
    {
      schema: buildSchema(HN_SCHEMA),
      uri: 'schema.graphql',
    },
  ],
});

window.MonacoEnvironment = {
  getWorker() {
    return new Worker(new URL('monaco-graphql/dist/graphql.worker.js', import.meta.url));
  },
};

const EXAMPLE_OPTIONS: { name: string; value: [string, string] }[] = [
  {
    name: 'Latest Stories',
    value: parseExample(latestStoriesExample),
  },
  {
    name: 'Top Stories',
    value: parseExample(topStoriesExample),
  },
  {
    name: 'Comments By patio11',
    value: parseExample(patio11Example),
  },
];

type QueryMessageEvent = MessageEvent<{ done: boolean; value: object }>;

export default function App(): JSX.Element {
  const [queryWorker, setQueryWorker] = useState<Worker | null>(null);
  const [fetcherWorker, setFetcherWorker] = useState<Worker | null>(null);
  const [ready, setReady] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [exampleQuery, setExampleQuery] = useState<{
    name: string;
    value: [string, string];
  } | null>(null);
  const queryEditorRef = useRef<HTMLDivElement>(null);
  const [queryEditor, setQueryEditor] = useState<monaco.editor.IStandaloneCodeEditor | null>(null);
  const varsEditorRef = useRef<HTMLDivElement>(null);
  const [varsEditor, setVarsEditor] = useState<monaco.editor.IStandaloneCodeEditor | null>(null);
  const resultsEditorRef = useRef<HTMLDivElement>(null);
  const [resultsEditor, setResultsEditor] = useState<monaco.editor.IStandaloneCodeEditor | null>(
    null
  );
  const [results, setResults] = useState<object[] | null>(null);
  const [nextResult, setNextResult] = useState<AsyncValue<object> | null>(null);

  const runQuery = useCallback(() => {
    if (queryWorker == null || queryEditor == null || varsEditor == null) return;
    const query = queryEditor.getValue();
    const vars = varsEditor.getValue();

    let varsObj = {};
    if (vars !== '') {
      try {
        varsObj = JSON.parse(vars ?? '');
      } catch (e) {
        setNextResult({
          status: 'error',
          error: `Error parsing variables to JSON:\n${(e as Error).message}`,
        });
        return;
      }
    }

    setHasMore(true);
    setNextResult({ status: 'pending' });

    queryWorker.postMessage({
      op: 'query',
      query,
      args: varsObj,
    });
  }, [queryWorker, queryEditor, varsEditor]);

  const queryNextResult = useCallback(() => {
    setNextResult({ status: 'pending' });
    queryWorker?.postMessage({
      op: 'next',
    });
  }, [queryWorker]);

  const handleQueryMessage = useCallback((evt: QueryMessageEvent) => {
    const outcome = evt.data;
    if (outcome.done) {
      setHasMore(false);
    } else {
      setNextResult({ status: 'ready', value: outcome.value });
      setHasMore(true);
    }
  }, []);

  const handleQueryError = useCallback((evt: ErrorEvent) => {
    setNextResult({
      status: 'error',
      error: `Error running query:\n${JSON.stringify(evt.message)}`,
    });
  }, []);

  const handleExampleQueryChange = useCallback((evt: SelectChangeEvent<string | null>) => {
    if (evt.target.value) {
      const example = EXAMPLE_OPTIONS.find((option) => option.name === evt.target.value) ?? null;
      setExampleQuery(example);
    }
  }, []);

  // Set example query
  useEffect(() => {
    if (exampleQuery && queryEditor && varsEditor) {
      const [query, vars] = exampleQuery.value;
      queryEditor.setValue(query);
      varsEditor.setValue(vars);
    }
  }, [exampleQuery, queryEditor, varsEditor]);

  // Init workers
  useEffect(() => {
    setQueryWorker(
      (prevWorker) =>
        prevWorker ?? new Worker(new URL('./adapter', import.meta.url), { type: 'module' })
    );
    setFetcherWorker(
      (prevWorker) =>
        prevWorker ?? new Worker(new URL('./fetcher', import.meta.url), { type: 'module' })
    );
  }, []);

  // Init editors
  useEffect(() => {
    if (queryEditorRef.current) {
      setQueryEditor(
        monaco.editor.create(
          queryEditorRef.current,
          {
            language: 'graphql',
            value: 'query {\n\n}',
            minimap: {
              enabled: false,
            },
            automaticLayout: true,
          },
          {
            storageService: {
              // eslint-disable-next-line @typescript-eslint/no-empty-function
              get() {},
              // Workaround to expand suggestion docs by default. See: https://stackoverflow.com/a/59040199
              getBoolean(key: string) {
                if (key === 'expandSuggestionDocs') return true;

                return false;
              },
              // eslint-disable-next-line @typescript-eslint/no-empty-function
              remove() {},
              // eslint-disable-next-line @typescript-eslint/no-empty-function
              store() {},
              // eslint-disable-next-line @typescript-eslint/no-empty-function
              onWillSaveState() {},
              // eslint-disable-next-line @typescript-eslint/no-empty-function
              onDidChangeStorage() {},
            },
          }
        )
      );
    }

    if (varsEditorRef.current) {
      setVarsEditor(
        monaco.editor.create(varsEditorRef.current, {
          language: 'json',
          value: '{\n\n}',
          minimap: {
            enabled: false,
          },
          automaticLayout: true,
          ...disableGutterConfig,
        })
      );
    }

    if (resultsEditorRef.current) {
      setResultsEditor(
        monaco.editor.create(resultsEditorRef.current, {
          language: 'json',
          value: '',
          minimap: {
            enabled: false,
          },
          readOnly: true,
          automaticLayout: true,
          ...disableGutterConfig,
        })
      );
    }
  }, []);

  // Update results editor
  useEffect(() => {
    if (resultsEditor) {
      if (nextResult && nextResult.status === 'error') {
        resultsEditor.setValue(nextResult.error);
      } else if (results == null) {
        resultsEditor.setValue('Run a query on the left to see results here.');
      } else {
        resultsEditor.setValue(JSON.stringify(results, null, 2));
      }

      const resultsEl = resultsEditorRef.current;
      if (resultsEl) {
        resultsEl.scrollTo(0, resultsEl.scrollHeight);
      }
    }
  }, [results, nextResult, resultsEditor]);

  // Set results
  useEffect(() => {
    if (nextResult && nextResult.status === 'ready') {
      setResults((prevResults) => {
        const prevValue = prevResults ? prevResults : [];
        return [...prevValue, nextResult.value];
      });
    } else if (nextResult == null || nextResult.status === 'error') {
      setResults(null);
    }
  }, [nextResult]);

  // Setup
  useEffect(() => {
    if (queryWorker == null || fetcherWorker == null) return;
    const channel = new MessageChannel();
    queryWorker.postMessage({ op: 'init' });

    fetcherWorker.postMessage({ op: 'channel', data: { port: channel.port2 } }, [channel.port2]);

    function awaitInitConfirmation(e: MessageEvent) {
      const data = e.data;
      if (data === 'ready' && queryWorker != null) {
        queryWorker.postMessage({ op: 'channel', data: { port: channel.port1 } }, [channel.port1]);

        queryWorker.removeEventListener('message', awaitInitConfirmation);
        queryWorker.addEventListener('message', handleQueryMessage);
        queryWorker.addEventListener('error', handleQueryError);
        setReady(true);
      } else {
        throw new Error(`Unexpected message: ${data}`);
      }
    }
    queryWorker.addEventListener('message', awaitInitConfirmation);

    return () => {
      queryWorker.removeEventListener('message', handleQueryMessage);
      queryWorker.removeEventListener('message', awaitInitConfirmation);
      setReady(false);
    };
  }, [fetcherWorker, queryWorker, handleQueryMessage, handleQueryError]);

  return (
    <Grid container direction="column" height="95vh" width="98vw" sx={{ flexWrap: 'nowrap' }}>
      <Grid item xs={1}>
        <Typography variant="h4" component="div">
          Trustfall in-browser query demo
        </Typography>
        <Typography>
          Query the HackerNews API directly from your browser with GraphQL, using{' '}
          <a href="https://github.com/obi1kenobi/trustfall" target="_blank" rel="noreferrer">
            Trustfall
          </a>{' '}
          compiled to WebAssembly.
        </Typography>
        <div css={{ display: 'flex', margin: 10 }}>
          <Button
            size="small"
            onClick={() => runQuery()}
            variant="contained"
            disabled={!ready}
            sx={{ mr: 2 }}
          >
            Run query!
          </Button>
          <FormControl size="small" sx={{ minWidth: 300 }}>
            <InputLabel id="example-query-label">Load an Example Query...</InputLabel>
            <Select
              labelId="example-query-label"
              value={exampleQuery ? exampleQuery.name : null}
              label="Load an Example Query..."
              onChange={handleExampleQueryChange}
            >
              {EXAMPLE_OPTIONS.map((option) => (
                <MenuItem key={option.name} value={option.name}>
                  {option.name}
                </MenuItem>
              ))}
            </Select>
          </FormControl>
        </div>
      </Grid>
      <Grid container item xs={11} spacing={2} sx={{ flexWrap: 'nowrap' }}>
        <Grid container item direction="column" xs={7} sx={{ flexWrap: 'nowrap' }}>
          <Grid container item direction="column" xs={8} sx={{ flexWrap: 'nowrap' }}>
            <Typography variant="h6" component="div">
              Query
            </Typography>
            <Paper elevation={0} sx={{ flexGrow: 1, position: 'relative', ...sxEditorContainer }}>
              <div ref={queryEditorRef} css={cssEditor} />
            </Paper>
          </Grid>
          <Grid container item direction="column" xs={4} sx={{ flexWrap: 'nowrap' }}>
            <Typography variant="h6" component="div" sx={{ mt: 1 }}>
              Variables
            </Typography>
            <Paper elevation={0} sx={{ flexGrow: 1, position: 'relative', ...sxEditorContainer }}>
              <div ref={varsEditorRef} css={cssEditor} />
            </Paper>
          </Grid>
        </Grid>
        <Grid container item xs={5} direction="column" sx={{ flexWrap: 'nowrap' }}>
          <Typography variant="h6" component="div">
            Results{' '}
            {(results != null || nextResult != null) && (
              <LoadingButton
                size="small"
                onClick={() => queryNextResult()}
                disabled={!hasMore}
                loading={nextResult != null && nextResult.status === 'pending'}
              >
                {hasMore ? 'More results!' : 'No more results'}
              </LoadingButton>
            )}
          </Typography>
          <Paper elevation={0} sx={{ flexGrow: 1, position: 'relative', ...sxEditorContainer }}>
            <div ref={resultsEditorRef} css={cssEditor} />
          </Paper>
        </Grid>
      </Grid>
    </Grid>
  );
}
