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

function* yieldMaterializedItems(fetchPort: MessagePort, itemIds: number[]): Generator<Item> {
  for (const id of itemIds) {
    const item = materializeItem(fetchPort, id);
    const itemType = item?.['type'];

    // Ignore polls. They are very rarely made on HackerNews,
    // and they are not supported in our query schema.
    if (itemType === 'story' || itemType === 'job') {
      yield item as Item;
    }
  }
}

function* resolveListOfItems(fetchPort: MessagePort, url: string): Generator<Item> {
  const sync = SyncContext.makeDefault();
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
  const itemIds = JSON.parse(result);

  yield* yieldMaterializedItems(fetchPort, itemIds);
}

export function* getTopItems(fetchPort: MessagePort): Generator<Item> {
  const url = 'https://hacker-news.firebaseio.com/v0/topstories.json';
  yield* resolveListOfItems(fetchPort, url);
}

export function* getLatestItems(fetchPort: MessagePort): Generator<Item> {
  const url = 'https://hacker-news.firebaseio.com/v0/newstories.json';
  yield* resolveListOfItems(fetchPort, url);
}

export function* getBestItems(fetchPort: MessagePort): Generator<Item> {
  const url = 'https://hacker-news.firebaseio.com/v0/beststories.json';
  yield* resolveListOfItems(fetchPort, url);
}

export function* getAskStories(fetchPort: MessagePort): Generator<Item> {
  const url = 'https://hacker-news.firebaseio.com/v0/askstories.json';
  yield* resolveListOfItems(fetchPort, url);
}

export function* getShowStories(fetchPort: MessagePort): Generator<Item> {
  const url = 'https://hacker-news.firebaseio.com/v0/showstories.json';
  yield* resolveListOfItems(fetchPort, url);
}

export function* getJobItems(fetchPort: MessagePort): Generator<Item> {
  const url = 'https://hacker-news.firebaseio.com/v0/jobstories.json';
  yield* resolveListOfItems(fetchPort, url);
}

export function* getUpdatedItems(fetchPort: MessagePort): Generator<Item> {
  const url = 'https://hacker-news.firebaseio.com/v0/updates.json';
  const sync = SyncContext.makeDefault();
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
  const itemIds = JSON.parse(result)?.items;

  yield* yieldMaterializedItems(fetchPort, itemIds);
}

export function* getUpdatedUserProfiles(fetchPort: MessagePort): Generator<User> {
  const url = 'https://hacker-news.firebaseio.com/v0/updates.json';
  const sync = SyncContext.makeDefault();
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
  const userIds = JSON.parse(result)?.profiles;

  for (const username of userIds) {
    const user = materializeUser(fetchPort, username);
    if (user) {
      yield user;
    }
  }
}

function* getSearchResults(
  fetchPort: MessagePort,
  endpoint: string,
  query: string
): Generator<Item> {
  const hitsPerPage = '50';
  let page = 0;

  while (true) {
    page += 1;
    const params = new URLSearchParams([
      ['query', query],
      ['page', page.toString()],
      ['hitsPerPage', hitsPerPage],
    ]);
    const url = `https://hn.algolia.com/api/v1/${endpoint}?${params}`;

    const sync = SyncContext.makeDefault();
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
    const hits = JSON.parse(result)?.hits;

    if (hits?.length) {
      for (const hit of hits) {
        const itemId = hit.objectID;
        if (itemId) {
          const item = materializeItem(fetchPort, itemId);
          if (item) {
            yield item;
          }
        }
      }
    } else {
      break;
    }
  }
}

export function* getRelevanceSearchResults(fetchPort: MessagePort, query: string): Generator<Item> {
  yield* getSearchResults(fetchPort, 'search', query);
}

export function* getDateSearchResults(fetchPort: MessagePort, query: string): Generator<Item> {
  yield* getSearchResults(fetchPort, 'search_by_date', query);
}
