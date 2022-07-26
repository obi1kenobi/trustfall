import { useCallback, useState, useEffect, useRef } from 'react';
import { buildSchema } from 'graphql';
import * as monaco from 'monaco-editor/esm/vs/editor/editor.api';
import { initializeMode } from 'monaco-graphql/esm/initializeMode';
import { css } from '@emotion/react';
import { Button, Grid, Paper, Typography } from '@mui/material';
import { HN_SCHEMA } from './adapter';

// Position absolute is necessary to keep the editor from growing constantly on window resize
// This is due to the height: 100% rule, since the container is slightly smaller
const cssEditor = css`
  position: absolute;
  top: 0;
  width: 100%;
  height: 100%;
  border: 1px solid #eee;
  border-radius: 5px;
`;

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

type QueryMessageEvent = MessageEvent<{ done: boolean; value: string }>;

export default function App(): JSX.Element {
  const [queryWorker, setQueryWorker] = useState<Worker | null>(null);
  const [fetcherWorker, setFetcherWorker] = useState<Worker | null>(null);
  const [ready, setReady] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const queryEditorRef = useRef<HTMLDivElement>(null);
  const [queryEditor, setQueryEditor] = useState<monaco.editor.IStandaloneCodeEditor | null>(null);
  const varsEditorRef = useRef<HTMLDivElement>(null);
  const [varsEditor, setVarsEditor] = useState<monaco.editor.IStandaloneCodeEditor | null>(null);
  const resultsEditorRef = useRef<HTMLDivElement>(null);
  const [resultsEditor, setResultsEditor] = useState<monaco.editor.IStandaloneCodeEditor | null>(
    null
  );
  const [results, setResults] = useState('');

  const runQuery = useCallback(() => {
    if (queryWorker == null || queryEditor == null || varsEditor == null) return;
    const query = queryEditor.getValue();
    const vars = varsEditor.getValue();

    let varsObj = {};
    if (vars !== '') {
      try {
        varsObj = JSON.parse(vars ?? '');
      } catch (e) {
        setResults(`Error parsing variables to JSON:\n${(e as Error).message}`);
        return;
      }
    }

    setHasMore(true);
    setResults('');

    queryWorker.postMessage({
      op: 'query',
      query,
      args: varsObj,
    });
  }, [queryWorker, queryEditor, varsEditor]);

  const queryNextResult = useCallback(() => {
    queryWorker?.postMessage({
      op: 'next',
    });
  }, [queryWorker]);

  const handleQueryMessage = useCallback((evt: QueryMessageEvent) => {
    const outcome = evt.data;
    if (outcome.done) {
      setResults((prevResults) =>
        prevResults.endsWith('***') ? prevResults : prevResults + '*** no more data ***'
      );
      setHasMore(false);
    } else {
      const pretty = JSON.stringify(outcome.value, null, 2);
      setResults((prevResults) => prevResults + `${pretty}\n`);
      setHasMore(true);
    }
    const resultsEl = resultsEditorRef.current;
    if (resultsEl) {
      resultsEl.scrollTo(0, resultsEl.scrollHeight);
    }
  }, []);

  const handleQueryError = useCallback((evt: ErrorEvent) => {
    setResults(`Error running query:\n${JSON.stringify(evt.message)}`);
  }, []);

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
        })
      );
    }
  }, []);

  // Update results
  useEffect(() => {
    if (resultsEditor) {
      resultsEditor.setValue(results);
    }
  }, [results, resultsEditor]);

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
    <Grid container direction="column" height="95vh" width="100vw" sx={{ flexWrap: 'nowrap' }}>
      <Grid item xs={1}>
        <Typography variant="h4" component="div">
          Trustfall in-browser query demo
        </Typography>
        <div css={{ margin: 10 }}>
          <Button onClick={() => runQuery()} variant="contained" disabled={!ready}>
            Run query!
          </Button>
          <Button onClick={() => queryNextResult()} disabled={!hasMore}>
            More results!
          </Button>
        </div>
      </Grid>
      <Grid container item xs={11} spacing={2} sx={{ flexWrap: 'nowrap' }}>
        <Grid container item direction="column" xs={8} sx={{ flexWrap: 'nowrap' }}>
          <Grid container item direction="column" xs={8} sx={{ flexWrap: 'nowrap' }}>
            <Typography variant="h6" component="div">
              Query
            </Typography>
            <Paper elevation={1} sx={{ flexGrow: 1, position: 'relative' }}>
              <div ref={queryEditorRef} css={cssEditor} />
            </Paper>
          </Grid>
          <Grid container item direction="column" xs={4} sx={{ flexWrap: 'nowrap' }}>
            <Typography variant="h6" component="div">
              Variables
            </Typography>
            <Paper elevation={1} sx={{ flexGrow: 1, position: 'relative' }}>
              <div ref={varsEditorRef} css={cssEditor} />
            </Paper>
          </Grid>
        </Grid>
        <Grid container item xs={4} direction="column" sx={{ flexWrap: 'nowrap' }}>
          <Typography variant="h6" component="div">
            Results
          </Typography>
          <Paper elevation={1} sx={{ flexGrow: 1, position: 'relative' }}>
            <div ref={resultsEditorRef} css={cssEditor} />
          </Paper>
        </Grid>
      </Grid>
    </Grid>
  );
}
