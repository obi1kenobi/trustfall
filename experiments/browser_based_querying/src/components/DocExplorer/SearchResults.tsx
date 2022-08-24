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
import { Divider, List, ListItem, Typography } from "@mui/material";
import React, { ReactNode } from "react";

import { useExplorerContext, useSchemaContext } from "@graphiql/react";
import Argument from "./Argument";
import FieldLink from "./FieldLink";
import styles from "./Styles";
import TypeLink from "./TypeLink";

const CategoryTitle: React.FC<{ title: string | null }> = ({ title }) => {
  return (
    <div style={{ textTransform: "uppercase" }}>
      <Typography sx={styles.searchResultTitles}>{title || ""}</Typography>
      <Divider />
    </div>
  );
};

export default function SearchResults() {
  const { explorerNavStack } = useExplorerContext({ nonNull: true });
  const { schema } = useSchemaContext({ nonNull: true });

  const navItem = explorerNavStack[explorerNavStack.length - 1];

  if (!schema || !navItem.search) {
    return null;
  }

  const searchValue = navItem.search;
  const withinType = navItem.def;

  const matchedWithin: ReactNode[] = [];
  const matchedTypes: ReactNode[] = [];
  const matchedFields: ReactNode[] = [];

  const typeMap = schema.getTypeMap();
  let typeNames = Object.keys(typeMap);

  // Move the within type name to be the first searched.
  if (withinType) {
    typeNames = typeNames.filter((n) => n !== withinType.name);
    typeNames.unshift(withinType.name);
  }

  for (const typeName of typeNames) {
    if (
      matchedWithin.length + matchedTypes.length + matchedFields.length >=
      100
    ) {
      break;
    }

    const type = typeMap[typeName];
    if (withinType !== type && isMatch(typeName, searchValue)) {
      matchedTypes.push(
        <ListItem key={typeName}>
          <TypeLink type={type} />
        </ListItem>
      );
    }

    if (type && "getFields" in type) {
      const fields = type.getFields();
      Object.keys(fields).forEach((fieldName) => {
        const field = fields[fieldName];
        let matchingArgs;

        if (!isMatch(fieldName, searchValue)) {
          if ("args" in field && field.args.length) {
            matchingArgs = field.args.filter((arg) =>
              isMatch(arg.name, searchValue)
            );
            if (matchingArgs.length === 0) {
              return;
            }
          } else {
            return;
          }
        }

        const match = (
          <ListItem key={typeName + "." + fieldName}>
            {withinType !== type && [<TypeLink key="type" type={type} />, "."]}
            <FieldLink field={field} />
            {matchingArgs && [
              "(",
              <span key="args">
                {matchingArgs.map((arg) => (
                  <Argument key={arg.name} arg={arg} showDefaultValue={false} />
                ))}
              </span>,
              ")",
            ]}
          </ListItem>
        );

        if (withinType === type) {
          matchedWithin.push(match);
        } else {
          matchedFields.push(match);
        }
      });
    }
  }

  if (matchedWithin.length + matchedTypes.length + matchedFields.length === 0) {
    return (
      <Typography sx={{ m: 1, fontWeight: "bold", color: "dimgray" }}>
        No results found
      </Typography>
    );
  }

  if (withinType && matchedTypes.length + matchedFields.length > 0) {
    return (
      <>
        <CategoryTitle title="Matched" />
        {matchedWithin}
        <CategoryTitle title="other results" />
        <List>{matchedTypes}</List>
        {matchedFields}
      </>
    );
  }

  return (
    <>
      <CategoryTitle title="Matched" />
      {matchedWithin}
      <List>{matchedTypes}</List>
      {matchedFields}
    </>
  );
}

function isMatch(sourceText: string, searchValue: string) {
  try {
    const escaped = searchValue.replace(/[^_0-9A-Za-z]/g, (ch) => "\\" + ch);
    return sourceText.search(new RegExp(escaped, "i")) !== -1;
  } catch (e) {
    return sourceText.toLowerCase().indexOf(searchValue.toLowerCase()) !== -1;
  }
}
