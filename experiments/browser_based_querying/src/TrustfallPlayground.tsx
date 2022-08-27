import { useCallback, useState, useEffect, useRef } from 'react';
import { GraphQLSchema } from 'graphql';
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
  Theme,
  SxProps,
} from '@mui/material';
import { LoadingButton } from '@mui/lab';

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
  } = props;
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
    <Grid container item direction="column" sx={{ flexWrap: 'nowrap', ...(sx ?? {})}}>
      <Grid item xs={1}>
        {header}
        <div css={{ display: 'flex', margin: 10 }}>
          <Button size="small" onClick={() => handleQuery()} variant="contained" sx={{ mr: 2 }}>
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
              {exampleQueries.map((option) => (
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
            {onQueryNextResult && results != null && (
              <LoadingButton
                size="small"
                onClick={() => onQueryNextResult()}
                disabled={!hasMore}
                loading={loading}
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
