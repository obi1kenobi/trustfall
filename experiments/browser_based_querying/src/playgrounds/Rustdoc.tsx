import { useCallback, useMemo, useState, useEffect } from 'react';
import { buildSchema } from 'graphql';
import { Autocomplete, Box, CircularProgress, Grid, TextField, Typography } from '@mui/material';

import { AsyncValue } from '../types';
import TrustfallPlayground from '../TrustfallPlayground';
import { CrateInfo, makeCrateInfo, runQuery } from '../../pkg/trustfall_rustdoc';
import rustdocSchema from '../../../trustfall_rustdoc/src/rustdoc_schema.graphql';

const RUSTDOC_SCHEMA = buildSchema(rustdocSchema);

import crateNames from '../rustdocCrates';

function fetchCrateJson(filename: string): Promise<string> {
  return fetch(
    `https://raw.githubusercontent.com/obi1kenobi/crates-rustdoc/main/max_version/${filename}.json`
  ).then((response) => response.text());
}

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
  crateInfo: CrateInfo;
}

function Playground(props: PlaygroundProps): JSX.Element {
  const { crateInfo } = props;
  const [results, setResults] = useState<object[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleQuery = useCallback((query: string, vars: string) => {
    let varsObj = {};
    if (vars !== '') {
      try {
        varsObj = JSON.parse(vars ?? '');
      } catch (e) {
        setError(
          `Error parsing variables to JSON:\n${(e as Error).message}`,
        );
        return;
      }
    }

    setLoading(true);
    // TODO: Run in a worker to avoid blocking the main thread;
    try {
      const results = runQuery(crateInfo, query, varsObj);
      setResults(results);
    } catch (message) {
      setError(`Error running query:\n${message}`)
    }
    setLoading(false);
  }, [crateInfo])

  return (
    <TrustfallPlayground
      results={results}
      loading={loading}
      error={error}
      hasMore={false}
      schema={RUSTDOC_SCHEMA}
      exampleQueries={[]}
      onQuery={handleQuery}
      sx={{height: "100%"}}
    />
  );
}

export default function Rustdoc(): JSX.Element {
  const [selectedCrate, setSelectedCrate] = useState<string | null>(null);
  const [asyncCrateJson, setAsyncCrateJson] = useState<AsyncValue<string> | null>(null);

  const crateInfo = useMemo(() => {
    if (asyncCrateJson?.status === 'ready') {
      return makeCrateInfo(asyncCrateJson.value);
    }
    return null;
  }, [asyncCrateJson]);

  const handleCrateChange = useCallback(
    (_evt: React.SyntheticEvent, option: CrateOption | null) => {
      setSelectedCrate(option?.value ?? null);
    },
    []
  );

  useEffect(() => {
    let current = true;
    if (selectedCrate != null) {
      setAsyncCrateJson({ status: 'pending' });

      fetchCrateJson(selectedCrate)
        .then((crateJson) => {
          if (current) {
            setAsyncCrateJson({ status: 'ready', value: crateJson });
          }
        })
        .catch(() => {
          if (current) {
            setAsyncCrateJson({
              status: 'error',
              error: 'Something went wrong while fetching crate info.',
            });
          }
        });
    } else {
      setAsyncCrateJson(null);
    }

    return () => {
      current = false;
    };
  }, [selectedCrate]);

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
        <Box>
          <Autocomplete
            options={CRATE_OPTIONS}
            renderInput={(params) => <TextField {...params} label="Choose a Crate" />}
            size="small"
            sx={{ width: 250 }}
            onChange={handleCrateChange}
          />
        </Box>
      </Box>
    );
  }, [handleCrateChange]);

  const playground = useMemo(() => {
    if (asyncCrateJson == null) return;

    if (asyncCrateJson.status === 'pending') {
      return <CircularProgress />;
    }

    if (asyncCrateJson.status === 'error') {
      return <Box sx={{ textAlign: 'center' }}>{asyncCrateJson.error}</Box>;
    }

    if (crateInfo) {
      return <Playground crateInfo={crateInfo} />;
    }

    return null;
  }, [crateInfo, asyncCrateJson]);

  return (
    <Grid container direction="column" height="98vh" width="98vw" sx={{flexWrap: "nowrap"}}>
      <Grid item xs={1}>
        {header}
      </Grid>
      <Grid container direction="column" item xs={11}>
        {playground}
      </Grid>
    </Grid>
  );
}
