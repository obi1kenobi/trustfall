/* Sets up a static doc explorer with a fixed schema. */
import { ExplorerContextProvider, SchemaContext, SchemaContextType } from '@graphiql/react';
import { GraphQLSchema } from 'graphql';
import React from 'react';
import { DocExplorer } from './DocExplorer';
import GRAPHQL_SCHEMA from './Schema';

const defaultSchemaContext: SchemaContextType = {
  fetchError: null,
  // eslint-disable-next-line @typescript-eslint/no-empty-function
  introspect() {},
  isFetching: false,
  schema: GRAPHQL_SCHEMA,
  validationErrors: [],
};

function DocExplorerWithContext(props: React.ComponentProps<typeof DocExplorer>) {
  return (
    <ExplorerContextProvider>
      <DocExplorer {...props} />
    </ExplorerContextProvider>
  );
}

const StaticDocExplorer: React.FC<{ schema: GraphQLSchema }> = ({ schema }) => {
  return (
    <SchemaContext.Provider
      value={{
        ...defaultSchemaContext,
        isFetching: false,
        schema: schema,
      }}
    >
      <DocExplorerWithContext />
    </SchemaContext.Provider>
  );
};

export default StaticDocExplorer;
