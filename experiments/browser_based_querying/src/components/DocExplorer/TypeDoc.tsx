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

import {
  ExplorerFieldDef,
  useExplorerContext,
  useSchemaContext,
} from "@graphiql/react";
import { Button, List, ListItem, Typography, Divider } from "@mui/material";
import {
  GraphQLEnumValue,
  GraphQLInterfaceType,
  GraphQLNamedType,
  GraphQLObjectType,
  isEnumType,
  isInterfaceType,
  isNamedType,
  isObjectType,
  isUnionType,
} from "graphql";
import { ReactNode, useState } from "react";

import Argument from "./Argument";
import DefaultValue from "./DefaultValue";
import FieldLink from "./FieldLink";
import MarkdownContent from "./MarkdownContent";
import TypeLink from "./TypeLink";
import styles from "./Styles";

const CategoryTitle: React.FC<{ title: string | null }> = ({ title }) => {
  return (
    <div style={{ textTransform: "uppercase" }}>
      <Typography sx={styles.typesTitle}>{title || ""}</Typography>
      <Divider />
    </div>
  );
};

export default function TypeDoc() {
  const { schema } = useSchemaContext({ nonNull: true });
  const { explorerNavStack } = useExplorerContext({ nonNull: true });
  const [showDeprecated, setShowDeprecated] = useState(false);

  const navItem = explorerNavStack[explorerNavStack.length - 1];
  const type = navItem.def;

  if (!schema || !isNamedType(type)) {
    return null;
  }

  let typesTitle: string | null = null;
  let types: readonly (GraphQLObjectType | GraphQLInterfaceType)[] = [];
  if (isUnionType(type)) {
    typesTitle = "Possible Types";
    types = schema.getPossibleTypes(type);
  } else if (isInterfaceType(type)) {
    typesTitle = "Implementations";
    types = schema.getPossibleTypes(type);
  } else if (isObjectType(type)) {
    typesTitle = "Implements";
    types = type.getInterfaces();
  }

  let typesDef;
  if (types && types.length > 0) {
    typesDef = (
      <div id="doc-types" className="doc-category">
        <CategoryTitle title={typesTitle} />
        <List dense>
          {types.map((subtype) => (
            <div key={subtype.name} className="doc-category-item">
              <ListItem>
                <TypeLink type={subtype} />
              </ListItem>
            </div>
          ))}
        </List>
      </div>
    );
  }

  // InputObject and Object
  let fieldsDef;
  let deprecatedFieldsDef;
  if (type && "getFields" in type) {
    const fieldMap = type.getFields();
    const fields = Object.keys(fieldMap).map((name) => fieldMap[name]);
    fieldsDef = (
      <div id="doc-fields" className="doc-category">
        <CategoryTitle title="Fields"/>
        <List dense>
          {fields
            .filter((field) => !field.deprecationReason)
            .map((field) => (
              <ListItem>
                <Field key={field.name} type={type} field={field} />
              </ListItem>
            ))}
        </List>
      </div>
    );

    const deprecatedFields = fields.filter((field) =>
      Boolean(field.deprecationReason)
    );
    if (deprecatedFields.length > 0) {
      deprecatedFieldsDef = (
        <div id="doc-deprecated-fields" className="doc-category">
          <div className="doc-category-title">
            <Typography fontWeight="bold">deprecated fields</Typography>
          </div>
          {!showDeprecated ? (
            <Button
              type="button"
              className="show-btn"
              onClick={() => {
                setShowDeprecated(true);
              }}
            >
              Show deprecated fields...
            </Button>
          ) : (
            deprecatedFields.map((field) => (
              <Field key={field.name} type={type} field={field} />
            ))
          )}
        </div>
      );
    }
  }

  let valuesDef: ReactNode;
  let deprecatedValuesDef: ReactNode;
  if (isEnumType(type)) {
    const values = type.getValues();
    valuesDef = (
      <div className="doc-category">
        <div className="doc-category-title">
          <Typography fontWeight="bold">values</Typography>
        </div>
        {values
          .filter((value) => Boolean(!value.deprecationReason))
          .map((value) => (
            <EnumValue key={value.name} value={value} />
          ))}
      </div>
    );

    const deprecatedValues = values.filter((value) =>
      Boolean(value.deprecationReason)
    );
    if (deprecatedValues.length > 0) {
      deprecatedValuesDef = (
        <div className="doc-category">
          <div className="doc-category-title">deprecated values</div>
          {!showDeprecated ? (
            <button
              type="button"
              className="show-btn"
              onClick={() => {
                setShowDeprecated(true);
              }}
            >
              Show deprecated values...
            </button>
          ) : (
            deprecatedValues.map((value) => (
              <EnumValue key={value.name} value={value} />
            ))
          )}
        </div>
      );
    }
  }

  return (
    <div>
      <MarkdownContent
        className="doc-type-description"
        markdown={
          ("description" in type && type.description) || "No Description"
        }
      />
      {isObjectType(type) && typesDef}
      {fieldsDef}
      {deprecatedFieldsDef}
      {valuesDef}
      {deprecatedValuesDef}
      {!isObjectType(type) && typesDef}
    </div>
  );
}

type FieldProps = {
  type: GraphQLNamedType;
  field: ExplorerFieldDef;
};

function Field({ field }: FieldProps) {
  return (
    <div className="doc-category-item">
      <FieldLink field={field} />
      {"args" in field &&
        field.args &&
        field.args.length > 0 && [
          <Typography display="inline">(</Typography>,
          field.args
            .filter((arg) => !arg.deprecationReason)
            .map((arg) => <Argument key={arg.name} arg={arg} />),
          <Typography display="inline">)</Typography>,
        ]}
      <Typography display="inline">: </Typography>
      <TypeLink type={field.type} />
      <DefaultValue field={field} />
      {field.description && (
        <MarkdownContent
          className="field-short-description"
          markdown={field.description}
        />
      )}
      {"deprecationReason" in field && field.deprecationReason && (
        <MarkdownContent
          className="doc-deprecation"
          markdown={field.deprecationReason}
        />
      )}
    </div>
  );
}

type EnumValueProps = {
  value: GraphQLEnumValue;
};

function EnumValue({ value }: EnumValueProps) {
  return (
    <div className="doc-category-item">
      <div className="enum-value">{value.name}</div>
      <MarkdownContent
        className="doc-value-description"
        markdown={value.description}
      />
      {value.deprecationReason && (
        <MarkdownContent
          className="doc-deprecation"
          markdown={value.deprecationReason}
        />
      )}
    </div>
  );
}
