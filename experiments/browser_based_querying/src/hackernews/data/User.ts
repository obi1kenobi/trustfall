import { Hackernews } from '../types';
import { extractPlainTextFromHnMarkup } from './DataUtils';
import { Webpage } from './Webpage';

type T = Hackernews['Firebase']['User'];

export class User extends Webpage {
  m_data: T;
  constructor(fetchPort: MessagePort, username: string, data: T) {
    super(fetchPort, `https://news.ycombinator.com/user?id=${username}`);
    this.m_data = data;
  }

  //   """
  //   The username of this user.
  //   """
  //   id: String!
  id(): number {
    return this.m_data.id;
  }

  //   """
  //   The user's accumulated karma points.
  //   """
  //   karma: Int!
  karma(): number {
    return this.m_data.karma;
  }

  //   """
  //   The HTML text the user has set in their "About" section, if any.
  //   """
  //   aboutHtml: String
  aboutHtml(): string | null {
    return this.m_data.about ?? null;
  }

  //   """
  //   The text the user has set in their "About" section, if any,
  //   as plain text with HTML tags removed.
  //   """
  //   aboutPlain: String
  aboutPlain(): string | null {
    const aboutHtml = this.m_data.about ?? null;

    if (aboutHtml === null) {
      return null;
    }

    return extractPlainTextFromHnMarkup(aboutHtml);
  }

  //   """
  //   The timestamp when the user account was created, as a number in Unix time.
  //   """
  //   unixCreatedAt: Int!
  unixCreatedAt(): number {
    return this.m_data.created;
  }

  //   """
  //   The URL of the user's HackerNews profile page.
  //   """
  //   url: String!
  /* inherited */

  //   # The HackerNews API treats submissions of comments and stories the same way.
  //   # The way to get only a user's submitted stories is to use this edge then
  //   # apply a type coercion on the `Item` vertex on edge endpoint:
  //   # `... on Story`
  //   """
  //   All submissions of this user, including all their stories and comments.

  //   To get a user's submitted stories, apply a type coercion to the edge:
  //   ```
  //   submitted {
  //     ... on Story {
  //       < query submitted stories here >
  //     }
  //   }
  //   ```
  //   """
  //   submitted: [Item!]

  //   """
  //   The web pages this user's "about" profile section links to, if any.
  //   """
  //   link: [Webpage!]
}
