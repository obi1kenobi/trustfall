/**
 *  Copyright (c) 2022 GraphQL Contributors.
 *
 *  This source code is licensed under the MIT license found in the
 *  LICENSE file.
 *
 *  This code has been slightly adapted to change the styling of elements.
 *  Original code is available here:
 *  Adapted from https://github.com/graphql/graphiql
 */
import { useSchemaContext } from '@graphiql/react';
import { Grid, Typography } from '@mui/material';
import MarkdownContent from './MarkdownContent';
import TypeLink from './TypeLink';

// Render the top level Schema
export default function SchemaDoc() {
  const { schema } = useSchemaContext({ nonNull: true });

  if (!schema) {
    return null;
  }

  const queryType = schema.getQueryType();
  const mutationType = schema.getMutationType?.();
  const subscriptionType = schema.getSubscriptionType?.();

  return (
    <Grid container direction="column" spacing={1} sx={{ m: 1 }}>
      <MarkdownContent
        className="doc-type-description"
        markdown={
          schema.description || 'A GraphQL schema provides a root type for each kind of operation.'
        }
      />
      <div className="doc-category">
        <Typography fontWeight="bold">Root Types</Typography>
        {queryType ? (
          <div className="doc-category-item">
            <Typography display="inline">query: </Typography>
            <TypeLink type={queryType} />
          </div>
        ) : null}
        {mutationType && (
          <div className="doc-category-item">
            <Typography display="inline">mutation: </Typography>
            <TypeLink type={mutationType} />
          </div>
        )}
        {subscriptionType && (
          <div className="doc-category-item">
            <Typography display="inline">subscription: </Typography>
            <TypeLink type={subscriptionType} />
          </div>
        )}
      </div>
    </Grid>
  );
}
