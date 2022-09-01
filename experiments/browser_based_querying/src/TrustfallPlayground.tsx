import { useCallback, useEffect, useRef, useState, useMemo } from 'react';
import { css } from '@emotion/react';
import { LoadingButton } from '@mui/lab';
import {
  Box,
  FormControl,
  Grid,
  InputLabel,
  MenuItem,
  Paper,
  Select,
  SelectChangeEvent,
  Tabs,
  Tab,
  Typography,
  Theme,
  SxProps,
  Tooltip,
} from '@mui/material';
import { GraphQLSchema } from 'graphql';
import * as monaco from 'monaco-editor/esm/vs/editor/editor.api';
import { initializeMode } from 'monaco-graphql/esm/initializeMode';
import { NumberParam, StringParam, useQueryParams } from 'use-query-params';

import SimpleDocExplorer from './components/SimpleDocExplorer';

const DEFAULT_ENCODING_FORMAT = 1;
const DEFAULT_QUERY = '';
const DEFAULT_VARS = '{\n\n}';

function decodeB64(str: string): string | null {
  try {
    return decodeURIComponent(escape(window.atob(str)));
  } catch {
    return null;
  }
}

function encodeB64(str: string): string {
  return window.btoa(unescape(encodeURIComponent(str)));
}

// Position absolute is necessary to keep the editor from growing constantly on window resize
// This is due to the height: 100% rule, since the container is slightly smaller
const cssEditor = css`
  position: absolute;
  top: 0;
  width: calc(100% - 5px);
  height: calc(100% - 3px);
`;

const sxEditorContainer: SxProps = {
  border: '1px solid #aaa',
  borderRadius: '5px',
  padding: '3px',
};

// Disable editor gutters entirely
const disableGutterConfig: monaco.editor.IStandaloneEditorConstructionOptions = {
  lineNumbers: 'off',
  glyphMargin: false,
  folding: false,
  // Undocumented see https://github.com/Microsoft/vscode/issues/30795#issuecomment-410998882
  lineDecorationsWidth: 5, // Leave a little padding on the left
  lineNumbersMinChars: 0,
};

window.MonacoEnvironment = {
  getWorker() {
    return new Worker(new URL('monaco-graphql/dist/graphql.worker.js', import.meta.url));
  },
};

type ResultsTab = 'results' | 'docs';

interface TabPanelProps {
  selected: boolean;
  children: React.ReactNode;
  sx?: SxProps;
}

function TabPanel(props: TabPanelProps): JSX.Element {
  const { selected, children, sx } = props;
  return (
    <Box sx={sx} hidden={!selected}>
      {children}
    </Box>
  );
}

interface TrustfallPlaygroundProps {
  results: object[] | null;
  loading: boolean;
  error: string | null;
  schema: GraphQLSchema;
  exampleQueries: { name: string; value: [string, string] }[];
  onQuery: (query: string, vars: string) => void;
  hasMore?: boolean;
  // Omit to hide "next result" button
  onQueryNextResult?: () => void;
  header?: React.ReactElement;
  sx?: SxProps<Theme>;
  disabled?: string | null; // Message to display if disabled
}

export default function TrustfallPlayground(props: TrustfallPlaygroundProps): JSX.Element {
  const {
    onQuery,
    onQueryNextResult,
    results,
    loading,
    error,
    hasMore,
    schema,
    exampleQueries,
    header,
    sx,
    disabled,
  } = props;
  const [queryParams, setQueryParams] = useQueryParams({
    f: NumberParam, // Format
    q: StringParam, // Query
    v: StringParam, // Vars
  });
  const { q: encodedQuery, v: encodedVars } = queryParams;

  // Use useState to grab the first value and cache it (unlike useMemo, which will update)
  const [initialQuery, _setInitialQuery] = useState<string>(
    () => decodeB64(encodedQuery ?? '') || (exampleQueries[0]?.value[0] ?? DEFAULT_QUERY)
  );
  const [initialVars, _setInitialVars] = useState<string>(
    () => decodeB64(encodedVars ?? '') || (exampleQueries[0]?.value[1] ?? DEFAULT_VARS)
  );
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
  const [selectedTab, setSelectedTab] = useState<ResultsTab>('results');

  const noQuery = encodedQuery === '';
  const disabledMessage = useMemo(() => {
    if (disabled) {
      return disabled;
    }

    if (noQuery) {
      return 'Write a query or load an example';
    }

    return '';
  }, [disabled, noQuery]);

  const handleTabChange = useCallback((_evt: React.SyntheticEvent, value: ResultsTab) => {
    setSelectedTab(value);
  }, []);

  const handleExampleQueryChange = useCallback(
    (evt: SelectChangeEvent<string | null>) => {
      if (evt.target.value) {
        const example = exampleQueries.find((option) => option.name === evt.target.value) ?? null;
        setExampleQuery(example);
      }
    },
    [exampleQueries]
  );

  const handleQuery = useCallback(() => {
    if (!queryEditor || !varsEditor) return;

    const query = queryEditor.getValue();
    const vars = varsEditor.getValue();

    onQuery(query, vars);
    setSelectedTab('results');
  }, [queryEditor, varsEditor, onQuery]);

  useEffect(() => {
    initializeMode({
      schemas: [
        {
          schema,
          uri: 'schema.graphql',
        },
      ],
    });
  }, [schema]);

  // Set example query
  useEffect(() => {
    if (exampleQuery && queryEditor && varsEditor) {
      const [query, vars] = exampleQuery.value;
      queryEditor.setValue(query);
      varsEditor.setValue(vars);
    }
  }, [exampleQuery, queryEditor, varsEditor]);

  // Init editors
  useEffect(() => {
    if (queryEditorRef.current && varsEditorRef.current && resultsEditorRef.current) {
      const queryEditor = monaco.editor.create(
        queryEditorRef.current,
        {
          language: 'graphql',
          value: initialQuery,
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
      );

      const varsEditor = monaco.editor.create(varsEditorRef.current, {
        language: 'json',
        value: initialVars,
        minimap: {
          enabled: false,
        },
        automaticLayout: true,
        ...disableGutterConfig,
      });
      const resultsEditor = monaco.editor.create(resultsEditorRef.current, {
          language: 'json',
          value: '',
          minimap: {
            enabled: false,
          },
          readOnly: true,
          automaticLayout: true,
          ...disableGutterConfig,
        })

      queryEditor.getModel()?.updateOptions({ tabSize: 2 })
      varsEditor.getModel()?.updateOptions({ tabSize: 2 })
      resultsEditor.getModel()?.updateOptions({ tabSize: 2 })
      setQueryEditor(queryEditor);
      setVarsEditor(varsEditor);
      setResultsEditor(resultsEditor);

      // Define inside effect to avoid infinite loop
      const updateQueryParams = () => {
        if (queryEditor && varsEditor) {
          setQueryParams(
            {
              f: DEFAULT_ENCODING_FORMAT,
              q: encodeB64(queryEditor.getValue()),
              v: encodeB64(varsEditor.getValue()),
            },
            'replaceIn'
          );
        }
      };

      updateQueryParams();
      const queryListener = queryEditor.onDidChangeModelContent(updateQueryParams);
      const varsListener = varsEditor.onDidChangeModelContent(updateQueryParams);
      return () => {
        queryListener.dispose();
        varsListener.dispose();
      };
    }

    return () => {}; // eslint-disable-line @typescript-eslint/no-empty-function
  }, [initialQuery, initialVars, setQueryParams]);

  // Update results editor
  useEffect(() => {
    if (resultsEditor) {
      if (error) {
        resultsEditor.setValue(error);
      } else if (results === null && loading) {
        resultsEditor.setValue('Loading...');
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
  }, [results, resultsEditor, loading, error]);

  return (
    <Grid container item direction="column" sx={{ flexWrap: 'nowrap', ...(sx ?? {}) }}>
      <Grid item xs={1}>
        {header}
        <div css={{ display: 'flex', alignItems: 'center', margin: 10 }}>
          <Tooltip title={disabledMessage} placement="bottom">
            <span>
              <LoadingButton
                size="small"
                onClick={() => handleQuery()}
                variant="contained"
                sx={{ mr: 2 }}
                disabled={Boolean(disabled) || noQuery}
                loading={loading}
              >
                Run query!
              </LoadingButton>
            </span>
          </Tooltip>
          {onQueryNextResult && results != null && (
            <LoadingButton
              size="small"
              variant="outlined"
              onClick={() => onQueryNextResult()}
              disabled={!hasMore || Boolean(disabled)}
              loading={loading}
              sx={{ mr: 2 }}
            >
              {hasMore ? 'More!' : 'No more results'}
            </LoadingButton>
          )}
          <FormControl size="small" sx={{ minWidth: 300 }}>
            <InputLabel id="example-query-label">Load an Example Query...</InputLabel>
            <Select
              labelId="example-query-label"
              value={exampleQuery ? exampleQuery.name : null}
              label="Load an Example Query..."
              onChange={handleExampleQueryChange}
            >
              {exampleQueries.map((option) => (
                <MenuItem key={option.name} value={option.name}>
                  {option.name}
                </MenuItem>
              ))}
            </Select>
          </FormControl>
        </div>
      </Grid>
      <Grid container item xs={11} spacing={2} sx={{ flexWrap: 'nowrap', overflowY: 'hidden' }}>
        <Grid container item direction="column" xs={7} sx={{ flexWrap: 'nowrap' }}>
          <Grid container item direction="column" xs={8} sx={{ flexWrap: 'nowrap' }}>
            {/* Use padding to align query section with results */}
            <Typography variant="overline" component="div" sx={{ pt: '1.5rem' }}>
              Query
            </Typography>
            <Paper elevation={0} sx={{ flexGrow: 1, position: 'relative', ...sxEditorContainer }}>
              <div ref={queryEditorRef} css={cssEditor} />
            </Paper>
          </Grid>
          <Grid container item direction="column" xs={4} sx={{ flexWrap: 'nowrap' }}>
            <Typography variant="overline" component="div" sx={{ mt: 1 }}>
              Variables
            </Typography>
            <Paper elevation={0} sx={{ flexGrow: 1, position: 'relative', ...sxEditorContainer }}>
              <div ref={varsEditorRef} css={cssEditor} />
            </Paper>
          </Grid>
        </Grid>
        <Grid container item xs={5} direction="column" sx={{ flexWrap: 'nowrap' }}>
          <Box>
            <Tabs value={selectedTab} onChange={handleTabChange} sx={{ pb: 1 }}>
              <Tab value="results" label="Results" />
              <Tab value="docs" label="Docs" />
            </Tabs>
          </Box>
          <TabPanel
            selected={selectedTab === 'results'}
            sx={{ flexGrow: 1, position: 'relative', ...sxEditorContainer }}
          >
            <Paper elevation={0}>
              <div ref={resultsEditorRef} css={cssEditor} />
            </Paper>
          </TabPanel>
          <TabPanel
            selected={selectedTab === 'docs'}
            sx={{
              display: selectedTab === 'docs' ? 'flex' : 'none',
              flexDirection: 'column',
              overflowY: 'hidden',
              overflowX: 'hidden',
            }}
          >
            <SimpleDocExplorer schema={schema} />
          </TabPanel>
        </Grid>
      </Grid>
    </Grid>
  );
}
