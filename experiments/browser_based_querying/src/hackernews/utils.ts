import { SyncContext } from '../sync';

interface Item {
  __typename: string;
  type: string;
}

interface User {
  __typename: string;
  id: string;
  created: number;
  karma: number;
  about: string | null; // HTML content
  submitted: number[] | null;
}

export function materializeItem(fetchPort: MessagePort, itemId: number): Item | null {
  const sync = SyncContext.makeDefault();

  const url = `https://hacker-news.firebaseio.com/v0/item/${itemId}.json`;
  const fetchOptions = {
    method: 'GET',
  };

  const message = {
    sync: sync.makeSendable(),
    input: url,
    init: fetchOptions,
  };
  fetchPort.postMessage(message);

  const result = new TextDecoder().decode(sync.receive());
  const item = JSON.parse(result);

  if (item) {
    switch (item.type) {
      case 'comment': {
        item.__typename = 'Comment';
        break;
      }
      case 'story': {
        item.__typename = 'Story';
        break;
      }
      case 'job': {
        item.__typename = 'Job';
        break;
      }
      default: {
        item.__typename = 'Item';
      }
    }
  }

  return item;
}

export function materializeUser(fetchPort: MessagePort, username: string): User | null {
  const sync = SyncContext.makeDefault();

  const url = `https://hacker-news.firebaseio.com/v0/user/${username}.json`;
  const fetchOptions = {
    method: 'GET',
  };

  const message = {
    sync: sync.makeSendable(),
    input: url,
    init: fetchOptions,
  };
  fetchPort.postMessage(message);

  const result = new TextDecoder().decode(sync.receive());
  const user = JSON.parse(result);

  if (user) {
    user.__typename = 'User';
  }

  return user;
}

export function* getTopItems(fetchPort: MessagePort): Generator<Item> {
  const sync = SyncContext.makeDefault();

  const url = 'https://hacker-news.firebaseio.com/v0/topstories.json';
  const fetchOptions = {
    method: 'GET',
  };

  const message = {
    sync: sync.makeSendable(),
    input: url,
    init: fetchOptions,
  };
  fetchPort.postMessage(message);

  const result = new TextDecoder().decode(sync.receive());
  const storyIds = JSON.parse(result);

  for (const id of storyIds) {
    const item = materializeItem(fetchPort, id);
    const itemType = item?.['type'];

    // Ignore polls. They are very rarely made on HackerNews,
    // and they are not supported in our query schema.
    if (itemType === 'story' || itemType === 'job') {
      yield item as Item;
    }
  }
}

export function* getLatestItems(fetchPort: MessagePort): Generator<Item> {
  const sync = SyncContext.makeDefault();

  const url = 'https://hacker-news.firebaseio.com/v0/newstories.json';
  const fetchOptions = {
    method: 'GET',
  };

  const message = {
    sync: sync.makeSendable(),
    input: url,
    init: fetchOptions,
  };
  fetchPort.postMessage(message);

  const result = new TextDecoder().decode(sync.receive());
  const storyIds = JSON.parse(result);

  for (const id of storyIds) {
    const item = materializeItem(fetchPort, id);
    const itemType = item?.['type'];

    // Ignore polls. They are very rarely made on HackerNews,
    // and they are not supported in our query schema.
    if (itemType === 'story' || itemType === 'job') {
      yield item as Item;
    }
  }
}
