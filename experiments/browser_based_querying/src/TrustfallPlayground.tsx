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
  useMediaQuery,
} from '@mui/material';
import { useTheme } from '@mui/material/styles';
import { GraphQLSchema } from 'graphql';
import * as monaco from 'monaco-editor/esm/vs/editor/editor.api';
import { initializeMode } from 'monaco-graphql/esm/initializeMode';
import { NumberParam, StringParam, useQueryParams } from 'use-query-params';
import { InPortal, OutPortal, createHtmlPortalNode } from 'react-reverse-portal';

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

const sxEditorBorder: SxProps = {
  border: '1px solid #aaa',
  borderRadius: '5px',
};

const sxEditorPadding: SxProps = {
  padding: '3px',
};

const sxEditorContainer: SxProps = {
  ...sxEditorBorder,
  ...sxEditorPadding,
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

const enableGutterConfig: monaco.editor.IStandaloneEditorConstructionOptions = {
  lineNumbers: 'on',
  glyphMargin: true,
  folding: true,
  lineDecorationsWidth: 5,
  lineNumbersMinChars: 2,
  acceptSuggestionOnEnter: "off",
  suggestOnTriggerCharacters: false,
};

window.MonacoEnvironment = {
  getWorker(_workerId: string, label: string) {
    switch (label) {
      case 'graphql':
        return new Worker(new URL('monaco-graphql/dist/graphql.worker.js', import.meta.url));
      case 'json':
        return new Worker(
          new URL('monaco-editor/esm/vs/language/json/json.worker', import.meta.url)
        );
      case 'editorWorkerService':
        return new Worker(new URL('monaco-editor/esm/vs/editor/editor.worker', import.meta.url));
      default:
        throw new Error(`No known worker for label ${label}`);
    }
  },
};

const OUTPUT_TABS = ['results', 'schema'] as const;
const INPUT_TABS = ['query', 'vars'] as const;
const TABS = [...INPUT_TABS, ...OUTPUT_TABS] as const;

type PlaygroundTab = typeof TABS[number];

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
    schema,
    error,
    hasMore,
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
  const [selectedTab, setSelectedTab] = useState<PlaygroundTab>('query');
  const queryEditorRef = useRef<HTMLDivElement>(null);
  const [queryEditor, setQueryEditor] = useState<monaco.editor.IStandaloneCodeEditor | null>(null);
  const varsEditorRef = useRef<HTMLDivElement>(null);
  const [varsEditor, setVarsEditor] = useState<monaco.editor.IStandaloneCodeEditor | null>(null);
  const resultsEditorRef = useRef<HTMLDivElement>(null);
  const [resultsEditor, setResultsEditor] = useState<monaco.editor.IStandaloneCodeEditor | null>(
    null
  );

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

  const handleExampleQueryChange = useCallback(
    (evt: SelectChangeEvent<string | null>) => {
      if (evt.target.value) {
        const example = exampleQueries.find((option) => option.name === evt.target.value) ?? null;
        setExampleQuery(example);
        setSelectedTab('query');
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

  const handleTabChange = useCallback((_evt: React.SyntheticEvent, value: PlaygroundTab) => {
    setSelectedTab(value);
  }, []);

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
        wordWrap: 'on',
        readOnly: true,
        automaticLayout: true,
        ...disableGutterConfig,
      });

      queryEditor.getModel()?.updateOptions({ tabSize: 2 });
      varsEditor.getModel()?.updateOptions({ tabSize: 2 });
      resultsEditor.getModel()?.updateOptions({ tabSize: 2 });
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

  // Force editors to re-render on tab change, necessary due to portals
  useEffect(() => {
    if (!queryEditor || !varsEditor || !resultsEditor) return;
    switch (selectedTab) {
      case 'query':
        queryEditor.layout();
        return;
      case 'vars':
        varsEditor.layout();
        return;
      case 'results':
        resultsEditor.layout();
        return;
    }
  }, [queryEditor, varsEditor, resultsEditor, selectedTab]);

  const queryPortalNode = useMemo(() => createHtmlPortalNode(), []);
  const varsPortalNode = useMemo(() => createHtmlPortalNode(), []);
  const resultsPortalNode = useMemo(() => createHtmlPortalNode(), []);
  const theme = useTheme();
  const isMdUp = useMediaQuery(theme.breakpoints.up('md'));

  const moreButton = useMemo(() => {
    if (onQueryNextResult && results != null) {
      return (
        <Box sx={{ ...sxEditorPadding, borderBottom: '1px solid #aaa' }}>
          <Grid item sx={{ mt: 1, mb: 1, textAlign: 'center' }}>
            <LoadingButton
              size="small"
              variant="outlined"
              onClick={() => onQueryNextResult()}
              disabled={!hasMore || Boolean(disabled)}
              loading={loading}
              sx={{ mr: 2 }}
            >
              {hasMore ? 'Fetch another result' : 'No more results'}
            </LoadingButton>
          </Grid>
        </Box>
      );
    }

    return null;
  }, [disabled, hasMore, loading, onQueryNextResult, results]);

  const mdUpContent = useMemo(
    () => (
      <>
        <Grid container item direction="column" md={7} sx={{ flexWrap: 'nowrap' }}>
          <Grid container item direction="column" md={8} sx={{ flexWrap: 'nowrap' }}>
            {/* Use padding to align query section with results */}
            <Typography variant="overline" component="div" sx={[{ pt: '1.5rem' }, false]}>
              Query
            </Typography>
            <Paper elevation={0} sx={{ flexGrow: 1, position: 'relative', ...sxEditorContainer }}>
              <OutPortal node={queryPortalNode} />
            </Paper>
          </Grid>
          <Grid container item direction="column" md={4} sx={{ flexWrap: 'nowrap' }}>
            <Typography variant="overline" component="div" sx={{ mt: 1 }}>
              Variables
            </Typography>
            <Paper elevation={0} sx={{ flexGrow: 1, position: 'relative', ...sxEditorContainer }}>
              <OutPortal node={varsPortalNode} />
            </Paper>
          </Grid>
        </Grid>
        <Grid container item md={5} direction="column" sx={{ flexWrap: 'nowrap' }}>
          <Box>
            <Tabs
              value={
                (OUTPUT_TABS as readonly string[]).includes(selectedTab) ? selectedTab : 'results'
              }
              onChange={handleTabChange}
              sx={{ pb: 1 }}
            >
              <Tab value="results" label="Results" />
              <Tab value="schema" label="Schema" />
            </Tabs>
          </Box>
          <TabPanel
            selected={
              (INPUT_TABS as readonly string[]).includes(selectedTab) || selectedTab === 'results'
            }
            sx={{ flexGrow: 1 }}
          >
            <Box
              sx={{
                height: '100%',
                display: 'flex',
                flexDirection: 'column',
                flexWrap: 'nowrap',
                ...sxEditorBorder,
              }}
            >
              {moreButton}
              <Box sx={{ flexGrow: 1, position: 'relative', ...sxEditorPadding }}>
                <Paper elevation={0}>
                  <OutPortal node={resultsPortalNode} />
                </Paper>
              </Box>
            </Box>
          </TabPanel>
          <TabPanel
            selected={selectedTab === 'schema'}
            sx={{
              display: selectedTab === 'schema' ? 'flex' : 'none',
              flexDirection: 'column',
              overflowY: 'hidden',
              overflowX: 'hidden',
            }}
          >
            <SimpleDocExplorer schema={schema} />
          </TabPanel>
        </Grid>
      </>
    ),
    [
      handleTabChange,
      queryPortalNode,
      varsPortalNode,
      resultsPortalNode,
      schema,
      selectedTab,
      moreButton,
    ]
  );

  const mdDownContent = useMemo(() => {
    const isQueryRelatedTab = selectedTab === 'query' || selectedTab === 'vars';
    const tabColor = isQueryRelatedTab ? 'secondary' : 'primary';
    return (
      <Grid
        container
        item
        xs={11}
        spacing={0}
        direction="column"
        sx={{ flexGrow: '1 !important', flexWrap: 'nowrap', overflowY: 'hidden' }}
      >
        <Box>
          <Tabs
            value={selectedTab}
            onChange={handleTabChange}
            textColor={tabColor}
            indicatorColor={tabColor}
            sx={{ pb: 1 }}
          >
            <Tab value="query" label="Query" />
            <Tab value="vars" label="Variables" />
            <Tab value="results" label="Results" />
            <Tab value="schema" label="Schema" />
          </Tabs>
        </Box>
        <TabPanel
          selected={selectedTab === 'query'}
          sx={{ flexGrow: 1, position: 'relative', ...sxEditorContainer }}
        >
          <Paper elevation={0}>
            <OutPortal node={queryPortalNode} />
          </Paper>
        </TabPanel>
        <TabPanel
          selected={selectedTab === 'vars'}
          sx={{ flexGrow: 1, position: 'relative', ...sxEditorContainer }}
        >
          <Paper elevation={0}>
            <OutPortal node={varsPortalNode} />
          </Paper>
        </TabPanel>
        <TabPanel selected={selectedTab === 'results'} sx={{ flexGrow: 1 }}>
          <Box
            sx={{
              height: '100%',
              display: 'flex',
              flexDirection: 'column',
              flexWrap: 'nowrap',
              ...sxEditorBorder,
            }}
          >
            {moreButton}
            <Box sx={{ flexGrow: 1, position: 'relative', ...sxEditorPadding }}>
              <Paper elevation={0}>
                <OutPortal node={resultsPortalNode} />
              </Paper>
            </Box>
          </Box>
        </TabPanel>
        <TabPanel
          selected={selectedTab === 'schema'}
          sx={{
            display: selectedTab === 'schema' ? 'flex' : 'none',
            flexDirection: 'column',
            flexGrow: 1,
            overflowY: 'hidden',
            overflowX: 'hidden',
          }}
        >
          <SimpleDocExplorer schema={schema} />
        </TabPanel>
      </Grid>
    );
  }, [
    handleTabChange,
    selectedTab,
    schema,
    queryPortalNode,
    varsPortalNode,
    resultsPortalNode,
    moreButton,
  ]);

  useEffect(() => {
    if (!queryEditor) return;

    if (isMdUp) {
      queryEditor.updateOptions(enableGutterConfig);
    } else {
      queryEditor.updateOptions(disableGutterConfig);
    }
  }, [isMdUp, queryEditor]);

  return (
    <Grid
      container
      item
      direction="column"
      spacing={0}
      sx={{ padding: '10px', flexWrap: 'nowrap', ...sx }}
    >
      <Grid item md={1}>
        {header}
        <Grid container item direction="row" spacing={0} sx={{ alignItems: 'center' }}>
          <Grid item sx={{ mt: 1, mr: '10px' }}>
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
          </Grid>
          <Grid item sx={{ mt: 1, mr: '10px' }}>
            <FormControl size="small" sx={{ minWidth: 300 }}>
              <InputLabel id="example-query-label">Load an Example Query...</InputLabel>
              <Select
                labelId="example-query-label"
                value={exampleQuery ? exampleQuery.name : ''}
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
          </Grid>
        </Grid>
      </Grid>
      {/* Use portals to prevent editors from being unmounted and remounted */}
      <InPortal node={queryPortalNode}>
        <div ref={queryEditorRef} css={cssEditor} />
      </InPortal>
      <InPortal node={varsPortalNode}>
        <div ref={varsEditorRef} css={cssEditor} />
      </InPortal>
      <InPortal node={resultsPortalNode}>
        <div ref={resultsEditorRef} css={cssEditor} />
      </InPortal>
      {isMdUp ? (
        <Grid
          container
          item
          md={11}
          spacing={2}
          sx={isMdUp ? { flexWrap: 'nowrap', overflowY: 'hidden' } : {}}
        >
          {mdUpContent}
        </Grid>
      ) : (
        <Grid
          container
          item
          xs={11}
          spacing={0}
          direction="column"
          sx={{ flexWrap: 'nowrap', flexGrow: 1, overflowY: 'hidden' }}
        >
          {mdDownContent}
        </Grid>
      )}
    </Grid>
  );
}
