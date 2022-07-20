import { useCallback, useState, useEffect, useRef } from 'react';
import { buildSchema } from 'graphql';
import * as monaco from 'monaco-editor/esm/vs/editor/editor.api';
import { initializeMode } from 'monaco-graphql/esm/initializeMode';
import { css } from '@emotion/react';
import { Button, Typography } from '@mui/material';
import { HN_SCHEMA } from './adapter';

const cssEditorContainer = css`
  flex-grow: 1;
  flex-shrink: 1;
`;

const cssEditor = css`
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
  const [resultsEditor, setResultsEditor] = useState<monaco.editor.IStandaloneCodeEditor | null>(null);
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
        monaco.editor.create(queryEditorRef.current, {
          language: 'graphql',
          value: 'query {\n\n}',
          minimap: {
            enabled: false,
          },
        })
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
        })
      );
    }
  }, []);

  // Update results
  useEffect(() => {
    if (resultsEditor) {
      resultsEditor.setValue(results);
    }
  }, [results, resultsEditor])

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
    <div>
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
      <div css={{ display: 'flex' }}>
        <div css={{ display: 'flex', flexDirection: 'column' }}>
          <div css={cssEditorContainer}>
            <Typography variant="h6" component="div">
              Query
            </Typography>
            <div ref={queryEditorRef} style={{ width: 800, height: 500 }} css={cssEditor} />
          </div>
          <div css={cssEditorContainer}>
            <Typography variant="h6" component="div">
              Variables
            </Typography>
            <div ref={varsEditorRef} style={{ width: 800, height: 300 }} css={cssEditor} />
          </div>
        </div>
        <div>
          <div
            ref={resultsEditorRef}
            css={{ width: 710, height: '100%' }}
          />
        </div>
      </div>
    </div>
  );
}
