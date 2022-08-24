/** Adapted from https://github.com/graphql/graphiql **/
import React from 'react';
import { astFromValue, print, ValueNode } from 'graphql';
import { ExplorerFieldDef } from '@graphiql/react';

const printDefault = (ast?: ValueNode | null): string => {
  if (!ast) {
    return '';
  }
  return print(ast);
};

type DefaultValueProps = {
  field: ExplorerFieldDef;
};

export default function DefaultValue({ field }: DefaultValueProps) {
  // field.defaultValue could be null or false, so be careful here!
  if ('defaultValue' in field && field.defaultValue !== undefined) {
    return (
      <span>
        {' = '}
        <span className="arg-default-value">
          {printDefault(astFromValue(field.defaultValue, field.type))}
        </span>
      </span>
    );
  }

  return null;
}
