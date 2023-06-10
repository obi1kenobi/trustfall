import { Hackernews } from '../types';
import { extractPlainTextFromHnMarkup } from './DataUtils';
import { Item } from './Item';

type T = Hackernews['Firebase']['Comment'];

export class Comment extends Item<T> {
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
  //   The text contained in the comment, represented as HTML.
  //   """
  //   textHtml: String!
  textHtml(): string | null {
    return this.m_data.text ?? null;
  }

  //   """
  //   The text contained in the comment, as plain text with HTML tags removed.
  //   """
  //   textPlain: String!
  textPlain(): string | null {
    return this.m_data.text ? extractPlainTextFromHnMarkup(this.m_data.text) : null;
  }

  //   """
  //   The name of the user that submitted this comment.
  //   """
  //   byUsername: String!
  byUsername(): string {
    return this.m_data.by;
  }

  //   """
  //   The profile of the user that submitted this comment.
  //   """
  //   byUser: User!

  //   """
  //   The replies to this comment, if any.
  //   """
  //   reply: [Comment!]

  //   """
  //   Links contained within the comment, if any.
  //   """
  //   link: [Webpage!]

  //   """
  //   The parent item: for top-level comments, this is the story or job
  //   where the comment was submitted, and for replies it's the comment
  //   which is being replied to.
  //   """
  //   parent: Item! # either a parent comment or the story being commented on
}
