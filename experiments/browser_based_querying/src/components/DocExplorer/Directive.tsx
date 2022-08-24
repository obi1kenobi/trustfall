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
import { DirectiveNode } from 'graphql';

type DirectiveProps = {
  directive: DirectiveNode;
};

export default function Directive({ directive }: DirectiveProps) {
  return (
    <span className="doc-category-item" id={directive.name.value}>
      @{directive.name.value}
    </span>
  );
}
