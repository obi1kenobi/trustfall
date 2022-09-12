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

  fetch(data.input, data.init)
    .then((response) => {
      if (!response.ok) {
        console.log('non-ok response:', response.status);
        sync.sendError(`non-ok response: ${response.status}`);
      } else {
        response
          .blob()
          .then((blob) => blob.arrayBuffer())
          .then((buffer) => {
            // The buffer we've received might not be a SharedArrayBuffer.
            // Defensively copy its contents into a new SharedArrayBuffer of appropriate size.
            const length = buffer.byteLength;
            const sharedArr = new Uint8Array(new SharedArrayBuffer(length));
            sharedArr.set(new Uint8Array(buffer), 0);

            sync.send(sharedArr);
          })
          .catch((reason) => {
            console.log('blob error:', response.status, reason);
            sync.sendError(`blob error: ${reason}`);
          });
      }
    })
    .catch((reason) => {
      console.log('fetch error:', reason);
      sync.sendError(`fetch error: ${reason}`);
    });
}
