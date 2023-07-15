import { useCallback, useMemo, useState, useLayoutEffect, useEffect } from 'react';
import { buildSchema } from 'graphql';
import { Autocomplete, Box, Grid, TextField, Typography } from '@mui/material';
import { StringParam, useQueryParam, withDefault } from 'use-query-params';

import { AsyncValue } from '../types';
import TrustfallPlayground from '../TrustfallPlayground';
import rustdocSchema from '../../../trustfall_rustdoc/src/rustdoc_schema.graphql';
import { RustdocWorkerResponse } from './types';
import parseExample from '../utils/parseExample';
import structNamesAndSpans from '../../example_queries/rustdoc/struct_names_and_spans.example';
import iterStructs from '../../example_queries/rustdoc/iter_structs.example';
import structsAndFields from '../../example_queries/rustdoc/structs_and_fields.example';
import enumsWithTupleVariants from '../../example_queries/rustdoc/enums_with_tuple_variants.example';
import itemsWithAllowedLints from '../../example_queries/rustdoc/items_with_allowed_lints.example';
import structsImportableByMultiplePaths from '../../example_queries/rustdoc/structs_importable_by_multiple_paths.example';
import traitsWithSupertraits from '../../example_queries/rustdoc/traits_with_supertraits.example';
import traitsWithAssociatedTypes from '../../example_queries/rustdoc/traits_with_associated_types.example';
import traitAssociatedConsts from '../../example_queries/rustdoc/trait_associated_consts.example';
import typeAssociatedConsts from '../../example_queries/rustdoc/type_associated_consts.example';

const RUSTDOC_SCHEMA = buildSchema(rustdocSchema);

const EXAMPLE_OPTIONS: { name: string; value: [string, string] }[] = [
  {
    name: 'Where Are Structs Defined?',
    value: parseExample(structNamesAndSpans),
  },
  {
    name: 'Structs Ending In "Iter"',
    value: parseExample(iterStructs),
  },
  {
    name: 'Listing Fields of Structs',
    value: parseExample(structsAndFields),
  },
  {
    name: 'Enums With Tuple Variants',
    value: parseExample(enumsWithTupleVariants),
  },
  {
    name: 'Items With Allowed Lints',
    value: parseExample(itemsWithAllowedLints),
  },
  {
    name: 'Structs Importable By Multiple Paths',
    value: parseExample(structsImportableByMultiplePaths),
  },
  {
    name: 'Traits With Supertraits',
    value: parseExample(traitsWithSupertraits),
  },
  {
    name: 'Traits With Associated Types',
    value: parseExample(traitsWithAssociatedTypes),
  },
  {
    name: 'Traits With Associated Constants',
    value: parseExample(traitAssociatedConsts),
  },
  {
    name: 'Structs And Enums With Associated Constants',
    value: parseExample(typeAssociatedConsts),
  },
];

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

const CrateParam = withDefault(StringParam, 'itertools-0.10.4')

interface PlaygroundProps {
  queryWorker: Worker;
  disabled: string | null;
}

function Playground(props: PlaygroundProps): JSX.Element {
  const { queryWorker, disabled } = props;
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
        setError(null);
        break;
      case 'query-error':
        setError(`Error: ${msg.message}`);
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
      exampleQueries={EXAMPLE_OPTIONS}
      onQuery={handleQuery}
      sx={{ height: '100%' }}
      disabled={disabled}
    />
  );
}

export default function Rustdoc(): JSX.Element {
  const [queryWorker, setQueryWorker] = useState<Worker | null>(null);
  const [selectedCrate, setSelectedCrate] = useQueryParam('crate', CrateParam);
  const [asyncLoadedCrate, setAsyncLoadedCrate] = useState<AsyncValue<string> | null>(null);
  const [workerReady, setWorkerReady] = useState(false);

  const handleCrateChange = useCallback(
    (_evt: React.SyntheticEvent, option: CrateOption | null) => {
      setSelectedCrate(option?.value ?? null, 'replaceIn');
    },
    [setSelectedCrate]
  );

  const handleWorkerMessage = useCallback((evt: MessageEvent<RustdocWorkerResponse>) => {
    const msg = evt.data;
    switch (msg.type) {
      case 'load-crate-ready':
        setAsyncLoadedCrate({ status: 'ready', value: msg.name });
        break;
      case 'load-crate-error':
        console.error(msg.message);
        setAsyncLoadedCrate({ status: 'error', error: msg.message });
        break;
      case 'ready':
        setWorkerReady(true);
    }
  }, []);

  const disabledMessage = useMemo(() => {
    if (!asyncLoadedCrate) {
      return 'First select a crate to query against';
    }

    if (asyncLoadedCrate.status === 'pending') {
      return 'Loading crate info...';
    }

    if (asyncLoadedCrate.status === 'error') {
      return 'Error loading crate, please try again';
    }

    return null;
  }, [asyncLoadedCrate]);

  useEffect(() => {
    setQueryWorker(
      (prevWorker) =>
        prevWorker ?? new Worker(new URL('./queryWorker', import.meta.url), { type: 'module' })
    );
  }, []);

  useEffect(() => {
    if (queryWorker && workerReady && selectedCrate && selectedCrate != '') {
      setAsyncLoadedCrate({ status: 'pending' });
      queryWorker.postMessage({
        op: 'load-crate',
        name: selectedCrate,
      });
    } else {
      setAsyncLoadedCrate(null);
    }
  }, [queryWorker, selectedCrate, workerReady]);

  // Register worker listener before all other effects are run
  useLayoutEffect(() => {
    if (!queryWorker) return;

    queryWorker.addEventListener('message', handleWorkerMessage);
    () => {
      queryWorker.removeEventListener('message', handleWorkerMessage);
    };
  }, [handleWorkerMessage, queryWorker]);

  const header = useMemo(() => {
    const selectedCrateOption = CRATE_OPTIONS.find((option) => option.value === selectedCrate) ?? null;
    return (
      <Box>
        <Typography variant="h4" component="div">
          Rust crates — Trustfall Playground
        </Typography>
        <Typography>
          Query a crate&apos;s rustdoc directly from your browser with{' '}
          <a href="https://github.com/obi1kenobi/trustfall" target="_blank" rel="noreferrer">
            Trustfall
          </a>{' '}
          compiled to WebAssembly.
        </Typography>
        <Typography>
          Selecting a crate downloads a few MB of data, so you might not want to do this from a
          mobile data plan. If your favorite crate is missing,{' '}
          <a href="https://github.com/obi1kenobi/crates-rustdoc/issues/new/choose">let us know</a>!
        </Typography>
        {queryWorker && (
          <Box>
            <Autocomplete
              options={CRATE_OPTIONS}
              renderInput={(params) => <TextField {...params} label="Choose a Crate" />}
              size="small"
              sx={{ mt: 2, width: 250 }}
              onChange={handleCrateChange}
              value={selectedCrateOption}
            />
          </Box>
        )}
      </Box>
    );
  }, [selectedCrate, queryWorker, handleCrateChange]);

  return (
    <Grid
      container
      direction="column"
      spacing={0}
      height="99vh"
      width="100vw"
      sx={{ flexWrap: 'nowrap' }}
    >
      <Grid item xs={1} sx={{ pt: 1, pl: 1, pr: 1 }}>
        {header}
      </Grid>
      <Grid
        container
        direction="column"
        item
        xs={11}
        spacing={0}
        sx={{ flexShrink: 1, overflowY: 'hidden' }}
      >
        {queryWorker && <Playground queryWorker={queryWorker} disabled={disabledMessage} />}
      </Grid>
    </Grid>
  );
}
