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
import MD from 'markdown-it';
import styles from './Styles';

const md = new MD({
  // render urls as links, à la github-flavored markdown
  breaks: true,
  linkify: true,
});

type MarkdownContentProps = {
  markdown?: string | null;
  className?: string;
};

export default function MarkdownContent({ markdown, className }: MarkdownContentProps) {
  if (!markdown) {
    return <div />;
  }

  return (
    <div
      style={styles.typeDocumentation}
      className={className}
      dangerouslySetInnerHTML={{ __html: md.render(markdown) }}
    />
  );
}
