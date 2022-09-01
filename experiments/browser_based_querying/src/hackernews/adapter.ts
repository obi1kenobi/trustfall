import init, {
  Adapter,
  JsEdgeParameters,
  JsContext,
  ContextAndValue,
  ContextAndNeighborsIterator,
  ContextAndBool,
  Schema,
  initialize,
  executeQuery,
} from '../../www2/trustfall_wasm.js';
import { getTopItems, getLatestItems, materializeItem, materializeUser } from './utils';
import schema from './schema.graphql';

// We need both of these!
await init();  // WASM system init.
initialize();  // Trustfall query system init.

export const HN_SCHEMA = schema;

Schema.parse(HN_SCHEMA);
console.log('Schema loaded.');

postMessage('ready');

type Vertex = any;

const HNItemFieldMappings: Record<string, string> = {
  id: 'id',
  unixTime: 'time',
  title: 'title',
  score: 'score',
  url: 'url',
  byUsername: 'by',
  text: 'text',
  commentsCount: 'descendants',
};

const HNUserFieldMappings: Record<string, string> = {
  id: 'id',
  karma: 'karma',
  about: 'about',
  unixCreatedAt: 'created',
};

function* limitIterator<T>(iter: IterableIterator<T>, limit: number): IterableIterator<T> {
  let count = 0;
  for (const item of iter) {
    yield item;
    count += 1;
    if (count == limit) {
      break;
    }
  }
}

export class MyAdapter implements Adapter<Vertex> {
  fetchPort: MessagePort;

  constructor(fetchPort: MessagePort) {
    this.fetchPort = fetchPort;
  }

  *getStartingTokens(edge: string, parameters: JsEdgeParameters): IterableIterator<Vertex> {
    if (edge === 'HackerNewsFrontPage') {
      return limitIterator(getTopItems(this.fetchPort), 30);
    } else if (edge === 'HackerNewsTop') {
      const limit = parameters['max'];
      const iter = getTopItems(this.fetchPort);
      if (limit == undefined) {
        yield* iter;
      } else {
        yield* limitIterator(iter, limit as number);
      }
    } else if (edge === 'HackerNewsLatestStories') {
      const limit = parameters['max'];
      const iter = getLatestItems(this.fetchPort);
      if (limit == undefined) {
        yield* iter;
      } else {
        yield* limitIterator(iter, limit as number);
      }
    } else if (edge === 'HackerNewsUser') {
      const username = parameters['name'];
      if (username == undefined) {
        throw new Error(`No username given: ${edge} ${parameters}`);
      }

      const user = materializeUser(this.fetchPort, username as string);
      if (user != null) {
        yield user;
      }
    } else {
      throw new Error(`Unexpected edge ${edge} with params ${parameters}`);
    }
  }

  *projectProperty(
    data_contexts: IterableIterator<JsContext<Vertex>>,
    current_type_name: string,
    field_name: string
  ): IterableIterator<ContextAndValue> {
    if (
      current_type_name === 'HackerNewsItem' ||
      current_type_name === 'HackerNewsStory' ||
      current_type_name === 'HackerNewsJob' ||
      current_type_name === 'HackerNewsComment'
    ) {
      if (field_name == 'ownUrl') {
        for (const ctx of data_contexts) {
          const vertex = ctx.currentToken;

          let value = null;
          if (vertex) {
            value = `https://news.ycombinator.com/item?id=${vertex.id}`;
          }

          yield {
            localId: ctx.localId,
            value: value,
          };
        }
      } else {
        const fieldKey = HNItemFieldMappings[field_name];
        if (fieldKey == undefined) {
          throw new Error(`Unexpected property for type ${current_type_name}: ${field_name}`);
        }

        for (const ctx of data_contexts) {
          const vertex = ctx.currentToken;

          yield {
            localId: ctx.localId,
            value: vertex ? vertex[fieldKey] || null : null,
          };
        }
      }
    } else if (current_type_name === 'HackerNewsUser') {
      const fieldKey = HNUserFieldMappings[field_name];
      if (fieldKey == undefined) {
        throw new Error(`Unexpected property for type ${current_type_name}: ${field_name}`);
      }

      for (const ctx of data_contexts) {
        const vertex = ctx.currentToken;
        yield {
          localId: ctx.localId,
          value: vertex ? vertex[fieldKey] || null : null,
        };
      }
    } else if (current_type_name === 'Webpage') {
      if (field_name === 'url') {
        for (const ctx of data_contexts) {
          const vertex = ctx.currentToken;
          yield {
            localId: ctx.localId,
            value: vertex?.url || null,
          };
        }
      } else {
        throw new Error(`Unexpected property: ${current_type_name} ${field_name}`);
      }
    } else {
      throw new Error(`Unexpected type+property for type ${current_type_name}: ${field_name}`);
    }
  }

  *projectNeighbors(
    data_contexts: IterableIterator<JsContext<Vertex>>,
    current_type_name: string,
    edge_name: string,
    parameters: JsEdgeParameters
  ): IterableIterator<ContextAndNeighborsIterator<Vertex>> {
    if (
      current_type_name === 'HackerNewsStory' ||
      current_type_name === 'HackerNewsJob' ||
      current_type_name === 'HackerNewsComment'
    ) {
      if (edge_name === 'link') {
        for (const ctx of data_contexts) {
          const vertex = ctx.currentToken;
          let neighbors: Vertex[] = [];
          if (vertex) {
            neighbors = [{ url: vertex.url }];
          }
          yield {
            localId: ctx.localId,
            neighbors: neighbors[Symbol.iterator](),
          };
        }
      } else if (edge_name === 'byUser') {
        for (const ctx of data_contexts) {
          const vertex = ctx.currentToken;
          if (vertex) {
            yield {
              localId: ctx.localId,
              neighbors: lazyFetchMap(this.fetchPort, [vertex.by], materializeUser),
            };
          } else {
            yield {
              localId: ctx.localId,
              neighbors: [][Symbol.iterator](),
            };
          }
        }
      } else if (edge_name === 'comment' || edge_name === 'reply') {
        for (const ctx of data_contexts) {
          const vertex = ctx.currentToken;
          yield {
            localId: ctx.localId,
            neighbors: lazyFetchMap(this.fetchPort, vertex?.kids, materializeItem),
          };
        }
      } else if (edge_name === 'parent') {
        for (const ctx of data_contexts) {
          const vertex = ctx.currentToken;
          const parent = vertex?.parent;
          if (parent) {
            yield {
              localId: ctx.localId,
              neighbors: lazyFetchMap(this.fetchPort, [parent], materializeItem),
            };
          } else {
            yield {
              localId: ctx.localId,
              neighbors: [][Symbol.iterator](),
            };
          }
        }
      } else {
        throw new Error(`Not implemented: ${current_type_name} ${edge_name} ${parameters}`);
      }
    } else if (current_type_name === 'HackerNewsUser') {
      if (edge_name === 'submitted') {
        for (const ctx of data_contexts) {
          const vertex = ctx.currentToken;
          const submitted = vertex?.submitted;
          yield {
            localId: ctx.localId,
            neighbors: lazyFetchMap(this.fetchPort, submitted, materializeItem),
          };
        }
      } else {
        throw new Error(`Not implemented: ${current_type_name} ${edge_name} ${parameters}`);
      }
    } else {
      throw new Error(`Not implemented: ${current_type_name} ${edge_name} ${parameters}`);
    }
  }

  *canCoerceToType(
    data_contexts: IterableIterator<JsContext<Vertex>>,
    current_type_name: string,
    coerce_to_type_name: string
  ): IterableIterator<ContextAndBool> {
    if (current_type_name === 'HackerNewsItem') {
      let targetType;
      if (coerce_to_type_name === 'HackerNewsStory') {
        targetType = 'story';
      } else if (coerce_to_type_name === 'HackerNewsJob') {
        targetType = 'job';
      } else if (coerce_to_type_name === 'HackerNewsComment') {
        targetType = 'comment';
      } else {
        throw new Error(`Unexpected coercion from ${current_type_name} to ${coerce_to_type_name}`);
      }

      for (const ctx of data_contexts) {
        const vertex = ctx.currentToken;
        yield {
          localId: ctx.localId,
          value: vertex?.type === targetType,
        };
      }
    } else {
      throw new Error(`Unexpected coercion from ${current_type_name} to ${coerce_to_type_name}`);
    }
  }
}

function* lazyFetchMap<InT, OutT>(
  fetchPort: MessagePort,
  inputs: Array<InT> | null,
  func: (port: MessagePort, arg: InT) => OutT
): IterableIterator<OutT> {
  if (inputs) {
    for (const input of inputs) {
      const result = func(fetchPort, input);
      if (result != null) {
        yield result;
      }
    }
  }
}

let _adapterFetchChannel: MessagePort;
let _resultIter: IterableIterator<object>;

function performQuery(query: string, args: Record<string, any>): IterableIterator<object> {
  if (query == null || query == undefined) {
    throw new Error(`Cannot perform null/undef query.`);
  }
  if (args == null || args == undefined) {
    throw new Error(`Cannot perform query with null/undef args.`);
  }

  // TODO: figure out why the schema object gets set to null
  //       as part of the executeQuery() call.
  const schemaCopy = Schema.parse(HN_SCHEMA);

  const adapter = new MyAdapter(_adapterFetchChannel);
  const resultIter = executeQuery(schemaCopy, adapter, query, args);

  return resultIter;
}

type AdapterMessage =
  | {
      op: 'init';
    }
  | {
      op: 'channel';
      data: {
        port: MessagePort;
      };
    }
  | {
      op: 'query';
      query: string;
      args: object;
    }
  | {
      op: 'next';
    };

function dispatch(e: MessageEvent<AdapterMessage>): void {
  const payload = e.data;

  console.log('Adapter received message:', payload);
  if (payload.op === 'init') {
    return;
  }

  if (payload.op === 'channel') {
    _adapterFetchChannel = payload.data.port;
    return;
  }

  if (payload.op === 'query') {
    _resultIter = performQuery(payload.query, payload.args);
  }

  if (payload.op === 'query' || payload.op === 'next') {
    const rawResult = _resultIter.next();
    const result = {
      done: rawResult.done,
      value: rawResult.value,
    };
    postMessage(result);
    return;
  }
}

onmessage = dispatch;
