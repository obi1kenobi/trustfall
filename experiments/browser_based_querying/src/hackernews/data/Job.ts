import { Hackernews } from '../types';
import { Item } from './Item';

type T = Hackernews['Firebase']['Job'];

export class Job extends Item<T> {
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
  //   The job posting's title: the one-liner seen on the front page, for example.
  //   """
  //   title: String!
  title(): string {
    return this.m_data.title;
  }

  //   """
  //   The total number of points this submission has received.
  //   """
  //   score: Int!
  score(): number {
    return this.m_data.score;
  }

  //   """
  //   The URL this job posting points to.
  //   """
  //   submittedUrl: String!
  submittedUrl(): string {
    return this.m_data.url;
  }

  //   # edges
  //   """
  //   The web page this job posting links to.
  //   """
  //   link: Webpage!
}
