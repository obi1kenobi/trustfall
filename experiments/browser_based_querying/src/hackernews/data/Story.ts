import { Hackernews } from '../types';
import { extractPlainTextFromHnMarkup } from './DataUtils';
import { Item } from './Item';

type T = Hackernews['Firebase']['Story'];

export class Story extends Item<T> {
  constructor(fetchPort: MessagePort, data: T) {
    super(fetchPort, data);
  }

  //   """
  //   The item's unique identifier.
  //   """
  //   id: Int!
  /* inherited */

  //   """
  //   The item's timestamp, as a number in Unix time.
  //   """
  //   unixTime: Int!
  /* inherited */

  //   """
  //   The item's URL on HackerNews.
  //   """
  //   url: String!
  /* inherited */

  //   """
  //   The display name of the user that submitted this story.
  //   """
  //   byUsername: String!
  byUsername(): string {
    return this.m_data.by;
  }

  //   """
  //   The current score of this story submission.
  //   """
  //   score: Int!
  score(): number {
    return this.m_data.score;
  }

  //   """
  //   For text submissions, contains the submitted text as HTML.
  //   For link submissions, this field is null.
  //   """
  //   textHtml: String
  textHtml(): string | null {
    return this.m_data.text ?? null;
  }

  //   """
  //   For text submissions, contains the submitted text as plain text,
  //   stripped of any HTML tags. For link submissions, this field is null.
  //   """
  //   textPlain: String
  textPlain(): string | null {
    return this.m_data.text ? extractPlainTextFromHnMarkup(this.m_data.text) : null;
  }

  //   """
  //   The story's title: the one-liner seen on the front page, for example.
  //   """
  //   title: String!
  title(): string {
    return this.m_data.title;
  }

  //   """
  //   For link submissions, contains the submitted link.
  //   For text submissions, this field is null.
  //   """
  //   submittedUrl: String
  submittedUrl(): string | null {
    return this.m_data.url ?? null;
  }

  //   """
  //   The profile of the user that submitted this story.
  //   """
  //   byUser: User!

  //   """
  //   The top-level comments on this story.
  //   """
  //   comment: [Comment!]

  //   """
  //   The web pages this story links to, if any.
  //   For link submissions, this is the submitted link.
  //   For text submissions, this includes all links in the text.
  //   """
  //   link: [Webpage!]
}
