import { useCallback, useState, useEffect, useRef } from 'react';
import Editor from '@monaco-editor/react';
import { css } from '@emotion/react';
import { Button, Typography } from '@mui/material';

const cssEditorContainer = css`
    flex-grow: 1;
    flex-shrink: 1;
`;

const cssEditor = css`
    border: 1px solid #eee;
    border-radius: 5px;
`

type QueryMessageEvent = MessageEvent<{ done: boolean; value: string }>;

export default function App(): JSX.Element {
  const [query, setQuery] = useState<string | undefined>('');
  const [vars, setVars] = useState<string | undefined>('');
  const [results, setResults] = useState('');
  const [queryWorker, setQueryWorker] = useState<Worker | null>(null);
  const [fetcherWorker, setFetcherWorker] = useState<Worker | null>(null);
  const [ready, setReady] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const resultsRef = useRef<HTMLTextAreaElement>(null);

  const runQuery = useCallback(() => {
    if (queryWorker == null) return;
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
  }, [query, queryWorker, vars]);

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
    // TODO: Scroll results textarea to bottom
    const resultsEl = resultsRef.current;
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
      <Typography variant="h4" component="div">Trustfall in-browser query demo</Typography>
      <div css={{ display: 'flex' }}>
        <div css={cssEditorContainer}>
          <Typography variant="h6" component="div">Query</Typography>
          <Editor
            defaultLanguage="graphql"
            value={query}
            onChange={setQuery}
            height="340px"
            options={{
              minimap: {
                enabled: false,
              },
            }}
            css={cssEditor}
          />
        </div>
        <div css={cssEditorContainer}>
          <Typography variant="h6" component="div">Variables</Typography>
          <Editor
            defaultLanguage="json"
            value={vars}
            onChange={setVars}
            height="340px"
            options={{
              minimap: {
                enabled: false,
              },
            }}
            css={cssEditor}
          />
        </div>
      </div>
      <div css={{ margin: 10 }}>
        <Button onClick={() => runQuery()} variant="contained" disabled={!ready}>
          Run query!
        </Button>
        <Button onClick={() => queryNextResult()} disabled={!hasMore}>
          More results!
        </Button>
      </div>
      <div>
        <textarea
          ref={resultsRef}
          value={results}
          css={{ width: 710, height: 300 }}
          readOnly
        ></textarea>
      </div>
    </div>
  );
}
