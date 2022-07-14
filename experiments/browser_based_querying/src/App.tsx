import { useCallback, useState, useEffect } from 'react';

type QueryMessageEvent = MessageEvent<{done: boolean, value: string}>

export default function App(): JSX.Element {
  const [query, setQuery] = useState('');
  const [vars, setVars] = useState('');
  const [results, setResults] = useState('');
  const [queryWorker, setQueryWorker] = useState<Worker | null>(null);
  const [fetcherWorker, setFetcherWorker] = useState<Worker | null>(null);
  const [ready, setReady] = useState(false);
  const [hasMore, setHasMore] = useState(false);

  const runQuery = useCallback(() => {
    if (queryWorker == null) return;
    let varsObj = {};
    if (vars !== '') {
      try {
        varsObj = JSON.parse(vars);
      } catch (e) {
        // TODO: Error messaging
        return;
      }
    }

    setHasMore(true);

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
    // TODO: Scroll results textarea to top
  }, []);

  // TODO: Handle error
  const handleQueryError = useCallback(() => {
    console.error('ERROR');
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
        queryWorker.addEventListener('messageerror', handleQueryError);
        setReady(true);
      } else {
        throw new Error(`Unexpected message: ${data}`);
      }
    }
    queryWorker.addEventListener('message', awaitInitConfirmation);

    return () => {
      queryWorker.removeEventListener('message', handleQueryMessage);
      queryWorker.removeEventListener('message', awaitInitConfirmation);
      queryWorker.removeEventListener('messageerror', handleQueryError);
      setReady(false);
      console.log("CLEANUP")
    };
  }, [fetcherWorker, queryWorker, handleQueryMessage, handleQueryError]);

  return (
    <div>
      <div>
        <textarea
          value={query}
          onChange={(evt) => setQuery(evt.target.value)}
          css={{ width: 500, height: 340 }}
        />
        <textarea
          value={vars}
          onChange={(evt) => setVars(evt.target.value)}
          css={{ width: 200, height: 340 }}
        ></textarea>
      </div>
      <div css={{ margin: 10 }}>
        <button onClick={() => runQuery()} disabled={!ready}>
          Run query!
        </button>
        <button onClick={() => queryNextResult()} disabled={!hasMore}>
          More results!
        </button>
      </div>
      <div>
        <textarea value={results} css={{ width: 710, height: 300 }} readOnly></textarea>
      </div>
    </div>
  );
}
