import { useCallback, useMemo, useState, useEffect } from 'react';
import { buildSchema } from 'graphql';
import { Autocomplete, Box, CircularProgress, Grid, TextField, Typography } from '@mui/material';

import { AsyncValue } from '../types';
import TrustfallPlayground from '../TrustfallPlayground';
import rustdocSchema from '../../../trustfall_rustdoc/src/rustdoc_schema.graphql';
import { RustdocWorkerResponse } from './types';

const RUSTDOC_SCHEMA = buildSchema(rustdocSchema);

import crateNames from '../rustdocCrates';

const fmtCrateName = (name: string): string => {
  const split = name.split('-');
  return `${split.slice(0, split.length - 1).join('-')} (${split[split.length - 1]})`;
};

interface CrateOption {
  label: string;
  value: string;
}

const CRATE_OPTIONS = crateNames.map((name) => ({ label: fmtCrateName(name), value: name }));

interface PlaygroundProps {
  queryWorker: Worker;
}

function Playground(props: PlaygroundProps): JSX.Element {
  const { queryWorker } = props;
  const [results, setResults] = useState<object[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleQuery = useCallback(
    (query: string, vars: string) => {
      let varsObj = {};
      if (vars !== '') {
        try {
          varsObj = JSON.parse(vars ?? '');
        } catch (e) {
          setError(`Error parsing variables to JSON:\n${(e as Error).message}`);
          return;
        }
      }

      setLoading(true);
      queryWorker.postMessage({ op: 'query', query, vars: varsObj });
    },
    [queryWorker]
  );

  const handleQueryMessage = useCallback((evt: MessageEvent<RustdocWorkerResponse>) => {
    const msg = evt.data;
    switch (msg.type) {
      case 'query-ready':
        setResults(msg.results);
        break;
      case 'query-error':
        setError(msg.message);
        break;
    }
    setLoading(false);
  }, []);

  const handleQueryError = useCallback((evt: ErrorEvent) => {
    setError(evt.message);
    setLoading(false);
  }, []);

  useEffect(() => {
    queryWorker.addEventListener('message', handleQueryMessage);
    queryWorker.addEventListener('error', handleQueryError);

    () => {
      queryWorker.removeEventListener('message', handleQueryMessage);
      queryWorker.removeEventListener('error', handleQueryError);
    };
  }, [handleQueryError, handleQueryMessage, queryWorker]);

  return (
    <TrustfallPlayground
      results={results}
      loading={loading}
      error={error}
      hasMore={false}
      schema={RUSTDOC_SCHEMA}
      exampleQueries={[]}
      onQuery={handleQuery}
      sx={{ height: '100%' }}
    />
  );
}

export default function Rustdoc(): JSX.Element {
  const [queryWorker, setQueryWorker] = useState<Worker | null>(null);
  const [selectedCrate, setSelectedCrate] = useState<string | null>(null);
  const [asyncLoadedCrate, setAsyncLoadedCrate] = useState<AsyncValue<string> | null>(null);

  const handleCrateChange = useCallback(
    (_evt: React.SyntheticEvent, option: CrateOption | null) => {
      setSelectedCrate(option?.value ?? null);
    },
    []
  );

  const handleWorkerMessage = useCallback((evt: MessageEvent<RustdocWorkerResponse>) => {
    const msg = evt.data;
    switch (msg.type) {
      case 'load-crate-ready':
        setAsyncLoadedCrate({ status: 'ready', value: msg.name });
        break;
      case 'load-crate-error':
        setAsyncLoadedCrate({ status: 'error', error: msg.message });
        break;
    }
  }, []);

  useEffect(() => {
    setQueryWorker(
      (prevWorker) =>
        prevWorker ?? new Worker(new URL('./queryWorker', import.meta.url), { type: 'module' })
    );
  }, []);

  useEffect(() => {
    if (queryWorker && selectedCrate) {
      setAsyncLoadedCrate({ status: 'pending' });
      queryWorker.postMessage({
        op: 'load-crate',
        name: selectedCrate,
      });
    } else {
      setAsyncLoadedCrate(null);
    }
  }, [queryWorker, selectedCrate]);

  useEffect(() => {
    if (!queryWorker) return;

    queryWorker.addEventListener('message', handleWorkerMessage);
    () => {
      queryWorker.removeEventListener('message', handleWorkerMessage);
    };
  }, [handleWorkerMessage, queryWorker]);

  const header = useMemo(() => {
    return (
      <Box>
        <Typography variant="h4" component="div">
          Trustfall in-browser query demo
        </Typography>
        <Typography>
          Query rustdocs directly from your browser with GraphQL, using{' '}
          <a href="https://github.com/obi1kenobi/trustfall" target="_blank" rel="noreferrer">
            Trustfall
          </a>{' '}
          compiled to WebAssembly.
        </Typography>
        {queryWorker && (
          <Box>
            <Autocomplete
              options={CRATE_OPTIONS}
              renderInput={(params) => <TextField {...params} label="Choose a Crate" />}
              size="small"
              sx={{ mt: 1, width: 250 }}
              onChange={handleCrateChange}
            />
          </Box>
        )}
      </Box>
    );
  }, [queryWorker, handleCrateChange]);

  const playground = useMemo(() => {
    if (asyncLoadedCrate == null) return;

    if (asyncLoadedCrate.status === 'pending' || !queryWorker) {
      return <CircularProgress />;
    }

    if (asyncLoadedCrate.status === 'error') {
      return <Box sx={{ textAlign: 'center' }}>{asyncLoadedCrate.error}</Box>;
    }

    if (asyncLoadedCrate.status === 'ready') {
      return <Playground queryWorker={queryWorker} />;
    }

    return null;
  }, [queryWorker, asyncLoadedCrate]);

  return (
    <Grid container direction="column" height="97vh" width="98vw" sx={{ flexWrap: 'nowrap' }}>
      <Grid item xs={1}>
        {header}
      </Grid>
      <Grid container direction="column" item xs={11}>
        {playground}
      </Grid>
    </Grid>
  );
}
