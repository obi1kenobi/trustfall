/** Adapted from https://github.com/graphql/graphiql **/
import MD from 'markdown-it';
import styles from "./Styles";

const md = new MD({
  // render urls as links, à la github-flavored markdown
  breaks: true,
  linkify: true,
});

type MarkdownContentProps = {
  markdown?: string | null;
  className?: string;
};

export default function MarkdownContent({
  markdown,
  className,
}: MarkdownContentProps) {
  if (!markdown) {
    return <div />;
  }

  return (
    <div style={styles.typeDocumentation}
      className={className}
      dangerouslySetInnerHTML={{ __html: md.render(markdown) }}
    />
  );
}
