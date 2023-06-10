import { decode } from 'html-entities';

export function extractPlainTextFromHnMarkup<T extends string | null>(hnText: T): T {
  // HN comments are not-quite-HTML: they support italics, links, paragraphs,
  // and preformatted text (code blocks), and use HTML escape sequences.
  // Docs: https://news.ycombinator.com/formatdoc
  if (hnText === null) return null as T;
  return decode(
    hnText
      .replaceAll('</a>', '') // remove closing link tags
      .replaceAll(/<a[^>]*>/g, '') // remove opening link tags
      .replaceAll(/<\/?(?:i|pre|code)>/g, '') // remove formatting tags
      .replaceAll('<p>', '\n') // turn paragraph tags into newlines
  ) as T;
}
