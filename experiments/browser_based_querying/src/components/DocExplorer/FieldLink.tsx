/**
 *  Copyright (c) 2022 GraphQL Contributors.
 *
 *  This source code is licensed under the MIT license found in the
 *  LICENSE file in the root directory of this source tree.
 */

import React from 'react';

import { ExplorerFieldDef, useExplorerContext } from '@graphiql/react';
import {Link} from '@mui/material';

type FieldLinkProps = {
  field: ExplorerFieldDef;
};

export default function FieldLink(props: FieldLinkProps) {
  const { push } = useExplorerContext({ nonNull: true });

  return (
    <Link
      onClick={event => {
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
