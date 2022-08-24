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
import { Typography } from '@mui/material';
import { GraphQLArgument } from 'graphql';
import DefaultValue from './DefaultValue';
import styles from './Styles';
import TypeLink from './TypeLink';

type ArgumentProps = {
  arg: GraphQLArgument;
  showDefaultValue?: boolean;
};

export default function Argument({ arg, showDefaultValue }: ArgumentProps) {
  return (
    <>
      <Typography display="inline" sx={styles.argument}>
        {arg.name}:{' '}
      </Typography>
      <TypeLink type={arg.type} />
      {showDefaultValue !== false && <DefaultValue field={arg} />}
    </>
  );
}
