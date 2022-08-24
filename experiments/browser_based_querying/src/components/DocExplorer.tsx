/**
 *  Copyright (c) 2022 GraphQL Contributors.
 *
 *  This source code is licensed under the MIT license found in the
 *  LICENSE file in the root directory of this source tree.
 *
 *  This code has been slightly adapted to change the styling of elements.
 *  Original code is available here:
 *  Adapted from https://github.com/graphql/graphiql
 */
import { useExplorerContext, useSchemaContext } from '@graphiql/react';
import SearchOffIcon from '@mui/icons-material/SearchOff';
import { Grid, Typography, Tooltip } from '@mui/material';
import Button from '@mui/material/Button';
import { GraphQLSchema, isType } from 'graphql';
import { ReactNode } from 'react';

import ArrowBackIosNewIcon from '@mui/icons-material/ArrowBackIosNew';
import FieldDoc from './DocExplorer/FieldDoc';
import SchemaDoc from './DocExplorer/SchemaDoc';
import SearchBox from './DocExplorer/SearchBox';
import SearchResults from './DocExplorer/SearchResults';
import TypeDoc from './DocExplorer/TypeDoc';

type DocExplorerProps = {
  onClose?(): void;
  /**
   * @deprecated Passing a schema prop directly to this component will be
   * removed in the next major version. Instead you need to wrap this component
   * with the `SchemaContextProvider` from `@graphiql/react`. This context
   * provider accepts a `schema` prop that you can use to skip fetching the
   * schema with an introspection request.
   */
  schema?: GraphQLSchema | null;
};

/**
 * DocExplorer
 *
 * Shows documentations for GraphQL definitions from the schema.
 *
 */
export function DocExplorer(props: DocExplorerProps) {
  const {
    fetchError,
    isFetching,
    schema: schemaFromContext,
    validationErrors,
  } = useSchemaContext({ nonNull: true });
  const { explorerNavStack, hide, pop, showSearch } = useExplorerContext({
    nonNull: true,
  });

  const navItem = explorerNavStack[explorerNavStack.length - 1];

  // The schema passed via props takes precedence until we remove the prop
  const schema = props.schema === undefined ? schemaFromContext : props.schema;

  let content: ReactNode = null;
  if (fetchError) {
    content = <Typography>Error fetching schema</Typography>;
  } else if (validationErrors.length > 0) {
    content = <Typography>Schema is invalid: {validationErrors[0].message}</Typography>;
  } else if (isFetching) {
    // Schema is undefined when it is being loaded via introspection.
    content = <Typography>Loading</Typography>;
  } else if (!schema) {
    // Schema is null when it explicitly does not exist, typically due to
    // an error during introspection.
    content = <Typography>No schema available</Typography>;
  } else if (navItem.search) {
    content = <SearchResults />;
  } else if (explorerNavStack.length === 1) {
    content = <SchemaDoc />;
  } else if (isType(navItem.def)) {
    content = <TypeDoc />;
  } else if (navItem.def) {
    content = <FieldDoc />;
  }

  console.log(explorerNavStack);

  const shouldSearchBoxAppear =
    explorerNavStack.length === 1 || (isType(navItem.def) && 'getFields' in navItem.def);

  let prevName;
  if (explorerNavStack.length > 1) {
    prevName = explorerNavStack[explorerNavStack.length - 2].name;
  }

  return (
    <Grid
      container
      aria-label="Documentation Explorer"
      key={navItem.name}
      direction="column"
      spacing={1}
    >
      <Grid container item direction="row" alignItems="center">
        <Grid item>
          <Button
            variant="text"
            startIcon={<ArrowBackIosNewIcon />}
            onClick={pop}
            aria-label={prevName ? `Go back to ${prevName}` : ''}
            sx={{ textTransform: 'none' }}
            disabled={prevName ? false : true}
          >
            {prevName ? '' : ''}
          </Button>
        </Grid>
        <Grid item>
          <Typography variant="body1" color="dimgray">{navItem.name}</Typography>
        </Grid>
      </Grid>
      <Grid item container direction="column">
        <Grid item>
          {shouldSearchBoxAppear && (
            <SearchBox
              value={navItem.search}
              placeholder={`Search ${navItem.name}...`}
              onSearch={showSearch}
            />
          )}
        </Grid>
        <Grid item container direction="column">
          {content}
        </Grid>
      </Grid>
    </Grid>
  );
}
