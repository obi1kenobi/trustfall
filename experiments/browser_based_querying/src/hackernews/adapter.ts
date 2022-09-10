import { decode } from 'html-entities';
import {
  Adapter,
  JsEdgeParameters,
  JsContext,
  ContextAndValue,
  ContextAndNeighborsIterator,
  ContextAndBool,
  Schema,
  initialize,
  executeQuery,
} from '../../www2/trustfall_wasm';
import { getTopItems, getLatestItems, materializeItem, materializeUser } from './utils';
import HN_SCHEMA from './schema.graphql';

initialize(); // Trustfall query system init.

const SCHEMA = Schema.parse(HN_SCHEMA);
console.log('Schema loaded.');

postMessage('ready');

type Vertex = any;

const HNItemFieldMappings: Record<string, string> = {
  id: 'id',
  unixTime: 'time',
  title: 'title',
  score: 'score',
  submittedUrl: 'url',
  byUsername: 'by',
  textHtml: 'text',
  // commentsCount: 'descendants',
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

function* linksInHnMarkup(fetchPort: MessagePort, hnText: string | null): IterableIterator<Vertex> {
  if (hnText) {
    const itemPattern = /^https:\/\/news\.ycombinator\.com\/item\?id=(\d+)$/;
    const userPattern = /^https:\/\/news\.ycombinator\.com\/user\?id=(.+)$/;

    const matches = hnText.matchAll(/<a [^>]*href="([^"]+)"[^>]*>/g);
    for (const match of matches) {
      // We matched the HTML-escaped URL. Decode the HTML entities.
      const url = decode(match[1]);
      const itemMatch = url.match(itemPattern);
      if (itemMatch) {
        // This is an item.
        yield materializeItem(fetchPort, parseInt(itemMatch[1]));
      } else {
        const userMatch = url.match(userPattern);
        if (userMatch) {
          // This is a user.
          yield materializeUser(fetchPort, userMatch[1]);
        } else {
          // This is some other type of webpage that we don't have a more specific type for.
          yield { url };
        }
      }
    }
  }
}

function extractPlainTextFromHnMarkup(hnText: string | null): string | null {
  // HN comments are not-quite-HTML: they support italics, links, paragraphs,
  // and preformatted text (code blocks), and use HTML escape sequences.
  // Docs: https://news.ycombinator.com/formatdoc
  if (hnText) {
    return decode(
      hnText
        .replaceAll('</a>', '') // remove closing link tags
        .replaceAll(/<a[^>]*>/g, '') // remove opening link tags
        .replaceAll(/<\/?(?:i|pre|code)>/g, '') // remove formatting tags
        .replaceAll('<p>', '\n') // turn paragraph tags into newlines
    );
  } else {
    return null;
  }
}

export class MyAdapter implements Adapter<Vertex> {
  fetchPort: MessagePort;

  constructor(fetchPort: MessagePort) {
    this.fetchPort = fetchPort;
  }

  *getStartingTokens(edge: string, parameters: JsEdgeParameters): IterableIterator<Vertex> {
    if (edge === 'FrontPage') {
      return limitIterator(getTopItems(this.fetchPort), 30);
    } else if (edge === 'Top') {
      const limit = parameters['max'];
      const iter = getTopItems(this.fetchPort);
      if (limit == undefined) {
        yield* iter;
      } else {
        yield* limitIterator(iter, limit as number);
      }
    } else if (edge === 'Latest') {
      const limit = parameters['max'];
      const iter = getLatestItems(this.fetchPort);
      if (limit == undefined) {
        yield* iter;
      } else {
        yield* limitIterator(iter, limit as number);
      }
    } else if (edge === 'User') {
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
      current_type_name === 'Item' ||
      current_type_name === 'Story' ||
      current_type_name === 'Job' ||
      current_type_name === 'Comment'
    ) {
      if (field_name == 'url') {
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
      } else if (field_name == 'textPlain') {
        const fieldKey = HNItemFieldMappings.textHtml;

        for (const ctx of data_contexts) {
          const vertex = ctx.currentToken;

          let value = null;
          if (vertex) {
            value = extractPlainTextFromHnMarkup(vertex[fieldKey]);
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
    } else if (current_type_name === 'User') {
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
      current_type_name === 'Story' ||
      current_type_name === 'Job' ||
      current_type_name === 'Comment'
    ) {
      if (edge_name === 'link') {
        if (current_type_name === 'Story') {
          // Link submission stories have the submitted URL as a link.
          // Text submission stories can have multiple links in the text.
          for (const ctx of data_contexts) {
            const vertex = ctx.currentToken;
            let neighbors: IterableIterator<Vertex>;
            if (vertex) {
              if (vertex.url) {
                // link submission
                neighbors = [{ url: vertex.url }][Symbol.iterator]();
              } else {
                // text submission
                neighbors = linksInHnMarkup(this.fetchPort, vertex.text);
              }
            } else {
              neighbors = [][Symbol.iterator]();
            }
            yield {
              localId: ctx.localId,
              neighbors,
            };
          }
        } else if (current_type_name === 'Comment') {
          // Comments can only have links in their text content.
          for (const ctx of data_contexts) {
            const vertex = ctx.currentToken;
            let neighbors: IterableIterator<Vertex>;
            if (vertex) {
              neighbors = linksInHnMarkup(this.fetchPort, vertex.text);
            } else {
              neighbors = [][Symbol.iterator]();
            }
            yield {
              localId: ctx.localId,
              neighbors,
            };
          }
        } else if (current_type_name === 'Job') {
          // Jobs only have the submitted URL as a link.
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
        } else {
          throw new Error(`Not implemented: ${current_type_name} ${edge_name} ${parameters}`);
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
    } else if (current_type_name === 'User') {
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
    if (current_type_name === 'Item' || current_type_name === 'Webpage') {
      if (coerce_to_type_name === 'Item') {
        // The Item type is abstract, we need to check if the vertex is any of the Item subtypes.
        for (const ctx of data_contexts) {
          const vertex = ctx.currentToken;
          const type = vertex?.type;
          yield {
            localId: ctx.localId,
            value: type === 'story' || type === 'job' || type === 'comment',
          };
        }
      } else {
        let targetType;
        if (coerce_to_type_name === 'Story') {
          targetType = 'story';
        } else if (coerce_to_type_name === 'Job') {
          targetType = 'job';
        } else if (coerce_to_type_name === 'Comment') {
          targetType = 'comment';
        } else {
          throw new Error(
            `Unexpected coercion from ${current_type_name} to ${coerce_to_type_name}`
          );
        }

        for (const ctx of data_contexts) {
          const vertex = ctx.currentToken;
          yield {
            localId: ctx.localId,
            value: vertex?.type === targetType,
          };
        }
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

  const adapter = new MyAdapter(_adapterFetchChannel);
  const resultIter = executeQuery(SCHEMA, adapter, query, args);

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
