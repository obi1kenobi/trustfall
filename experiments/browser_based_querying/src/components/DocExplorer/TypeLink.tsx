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
import { useExplorerContext } from '@graphiql/react';
import { Link } from '@mui/material';
import { GraphQLType, isListType, isNonNullType } from 'graphql';
import styles from './Styles';

type TypeLinkProps = {
  type: GraphQLType;
};

export default function TypeLink(props: TypeLinkProps) {
  const { push } = useExplorerContext({ nonNull: true, caller: TypeLink });

  if (!props.type) {
    return null;
  }

  const type = props.type;
  if (isNonNullType(type)) {
    return (
      <>
        <TypeLink type={type.ofType} />!
      </>
    );
  }
  if (isListType(type)) {
    return (
      <>
        [<TypeLink type={type.ofType} />]
      </>
    );
  }
  return (
    <Link
      onClick={(event) => {
        event.preventDefault();
        push({ name: type.name, def: type });
      }}
      href="#"
      underline="none"
      sx={styles.type}
    >
      {type.name}
    </Link>
  );
}
