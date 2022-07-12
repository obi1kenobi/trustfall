import { SyncContext } from "./sync";

onmessage = function (e): void {
  let data = e.data;
  console.log("Fetcher received data:", data);

  if (data.op === "channel") {
    const inputChannel = data.data.port;
    inputChannel.onmessage = fetchHandler;
    return;
  }

  throw new Error(data);
}

function fetchHandler(e: any): void {
  let data = e.data;
  console.log("Fetcher received channel data:", data);

  const sync = new SyncContext(data.sync);

  fetch(data.input, data.init)
    .then((response) => {
      console.log("worker fetch complete:", response.ok, response.status);
      if (!response.ok) {
        console.log("non-ok response:", response.status);
        sync.sendError(`non-ok response: ${response.status}`);
      } else {
        response.blob()
          .then((blob) => blob.arrayBuffer())
          .then((buffer) => {
            sync.send(new Uint8Array(buffer));
          })
          .catch((reason) => {
            console.log("blob error:", response.status);
            sync.sendError(`blob error: ${reason}`);
          })
      }
    })
    .catch((reason) => {
      console.log("fetch error:", reason);
      sync.sendError(`fetch error: ${reason}`);
    });
}
