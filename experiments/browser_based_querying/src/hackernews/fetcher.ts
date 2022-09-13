import debug from "../utils/debug";
import { SendableSyncContext, SyncContext } from '../sync';

interface ChannelData {
  input: string;
  init: {
    method: 'GET';
  };
  sync: SendableSyncContext;
}

onmessage = function (e): void {
  const data = e.data;

  if (data.op === 'channel') {
    const inputChannel = data.data.port;
    inputChannel.onmessage = fetchHandler;
    return;
  }

  throw new Error(data);
};

function fetchHandler(e: MessageEvent<ChannelData>): void {
  const data = e.data;

  const sync = new SyncContext(data.sync);

  debug('Fetcher received message:', data);
  fetch(data.input, data.init)
    .then((response) => {
      if (!response.ok) {
        debug('non-ok response:', response.status);
        sync.sendError(`non-ok response: ${response.status}`);
      } else {
        response
          .blob()
          .then((blob) => blob.arrayBuffer())
          .then((buffer) => {
            sync.send(new Uint8Array(buffer));
          })
          .catch((reason) => {
            debug('blob error:', response.status, reason);
            sync.sendError(`blob error: ${reason}`);
          });
      }
    })
    .catch((reason) => {
      debug('fetch error:', reason);
      sync.sendError(`fetch error: ${reason}`);
    });
}
