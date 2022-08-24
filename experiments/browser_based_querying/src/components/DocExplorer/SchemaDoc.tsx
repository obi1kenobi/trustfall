/** Adapted from https://github.com/graphql/graphiql **/
import React from "react";
import TypeLink from "./TypeLink";
import MarkdownContent from "./MarkdownContent";
import { useSchemaContext } from "@graphiql/react";
import { Grid, Typography } from "@mui/material";

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
          schema.description ||
          "A GraphQL schema provides a root type for each kind of operation."
        }
      />
      <div className="doc-category">
        <Typography fontWeight="bold">root types</Typography>
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
