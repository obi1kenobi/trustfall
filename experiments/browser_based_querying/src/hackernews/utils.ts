import { SyncContext } from '../sync';

interface HackerNewsItem {
  type: string;
}

export function materializeItem(fetchPort: MessagePort, itemId: number): HackerNewsItem {
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
  console.log('materialized item:', item);

  return item;
}

export function materializeUser(fetchPort: MessagePort, username: string): unknown {
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
  console.log('materialized user:', user);

  return user;
}

export function* getTopItems(fetchPort: MessagePort): Generator<HackerNewsItem> {
  const sync = SyncContext.makeDefault();

  const url = 'https://hacker-news.firebaseio.com/v0/topstories.json';
  const fetchOptions = {
    method: 'GET',
    // "credentials": "omit",
  };

  console.log('posting to fetcher');
  const message = {
    sync: sync.makeSendable(),
    input: url,
    init: fetchOptions,
  };
  fetchPort.postMessage(message);

  console.log('waiting (1) for fetcher');

  const result = new TextDecoder().decode(sync.receive());
  console.log('result=', result);
  const storyIds = JSON.parse(result);
  console.log('storyIds=', storyIds);

  for (const id of storyIds) {
    const item = materializeItem(fetchPort, id);
    const itemType = item['type'];

    // Ignore polls. They are very rarely made on HackerNews,
    // and they are not supported in our query schema.
    if (itemType === 'story' || itemType === 'job') {
      yield item;
    }
  }
}

export function* getLatestItems(fetchPort: MessagePort): Generator<HackerNewsItem> {
  const sync = SyncContext.makeDefault();

  const url = 'https://hacker-news.firebaseio.com/v0/newstories.json';
  const fetchOptions = {
    method: 'GET',
    // "credentials": "omit",
  };

  console.log('posting to fetcher');
  const message = {
    sync: sync.makeSendable(),
    input: url,
    init: fetchOptions,
  };
  fetchPort.postMessage(message);

  console.log('waiting (1) for fetcher');

  const result = new TextDecoder().decode(sync.receive());
  console.log('result=', result);
  const storyIds = JSON.parse(result);
  console.log('storyIds=', storyIds);

  for (const id of storyIds) {
    const item = materializeItem(fetchPort, id);
    const itemType = item['type'];

    // Ignore polls. They are very rarely made on HackerNews,
    // and they are not supported in our query schema.
    if (itemType === 'story' || itemType === 'job') {
      yield item;
    }
  }
}
