import { ExplorerContextProvider, SchemaContext, SchemaContextType } from '@graphiql/react';
import { GraphQLSchema } from 'graphql';
import React from 'react';
import { DocExplorer } from './DocExplorer';

const defaultSchemaContext: SchemaContextType = {
  fetchError: null,
  // eslint-disable-next-line @typescript-eslint/no-empty-function
  introspect() {},
  isFetching: true,
  schema: null,
  validationErrors: [],
};

function DocExplorerWithContext(props: React.ComponentProps<typeof DocExplorer>) {
  return (
    <ExplorerContextProvider>
      <DocExplorer {...props} />
    </ExplorerContextProvider>
  );
}

const SimpleDocExplorer: React.FC<{ schema: GraphQLSchema }> = ({ schema }) => {
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

export default SimpleDocExplorer;
