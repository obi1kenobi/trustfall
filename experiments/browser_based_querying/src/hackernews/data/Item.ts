import { Hackernews } from '../types';
import { Webpage } from './Webpage';

type T = Hackernews['Firebase']['Item'];

export class Item<Data extends T = T> extends Webpage {
  protected m_data: Data;
  constructor(fetchPort: MessagePort, data: Data) {
    super(fetchPort, `https://news.ycombinator.com/item?id=${data.id}`);
    this.m_data = data;
  }

  //   """
  //   The item's unique identifier.
  //   """
  //   id: Int!
  id(): number {
    return this.m_data.id;
  }

  //   """
  //   The item's timestamp, as a number in Unix time.
  //   """
  //   unixTime: Int!
  unixTime(): number {
    return this.m_data.time;
  }

  //   """
  //   The item's URL on HackerNews.
  //   """
  //   url: String!

  /* inherited */

  /**
   * NOT EXPOSED TO THE GRAPHQL SCHEMA
   * @returns The type of this item.
   */
  type(): string {
    return this.m_data.type;
  }
}
