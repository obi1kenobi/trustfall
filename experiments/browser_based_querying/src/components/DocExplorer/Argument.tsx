/**
 *  Copyright (c) 2021 GraphQL Contributors.
 *
 *  This source code is licensed under the MIT license found in the
 *  LICENSE file in the root directory of this source tree.
 */

import { Typography } from '@mui/material';
import { GraphQLArgument } from 'graphql';
import DefaultValue from './DefaultValue';
import TypeLink from './TypeLink';
import styles from './Styles';

type ArgumentProps = {
  arg: GraphQLArgument;
  showDefaultValue?: boolean;
};

export default function Argument({ arg, showDefaultValue }: ArgumentProps) {
  return (
    <>
      <Typography display="inline" sx={styles.argument}>{arg.name}: </Typography>
      <TypeLink type={arg.type} />
      {showDefaultValue !== false && <DefaultValue field={arg} />}
    </>
  );
}
