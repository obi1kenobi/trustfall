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

import { ExplorerFieldDef, useExplorerContext } from '@graphiql/react';
import { Link } from '@mui/material';

type FieldLinkProps = {
  field: ExplorerFieldDef;
};

export default function FieldLink(props: FieldLinkProps) {
  const { push } = useExplorerContext({ nonNull: true });

  return (
    <Link
      onClick={(event) => {
        event.preventDefault();
        push({ name: props.field.name, def: props.field });
      }}
      href="#"
      underline="none"
    >
      {props.field.name}
    </Link>
  );
}
