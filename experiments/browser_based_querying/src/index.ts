const queryBox = document.getElementById("query")! as HTMLTextAreaElement;
const varsBox = document.getElementById("vars")! as HTMLTextAreaElement;

const runButton = document.getElementById("run")! as HTMLButtonElement;
const moreButton = document.getElementById("more")! as HTMLButtonElement;

const resultsBox = document.getElementById("results")! as HTMLTextAreaElement;

const queryWorker = new Worker(new URL("./adapter", import.meta.url), { type: "module" });
const fetcherWorker = new Worker(new URL("./fetcher", import.meta.url), { type: "module" });

function setup(then: () => void): void {
  const channel = new MessageChannel();
  queryWorker.postMessage({ op: "init" });

  fetcherWorker.postMessage(
    { op: "channel", data: { port: channel.port2 } }, [channel.port2],
  );

  function cleanUp(): any {
    queryWorker.removeEventListener('message', awaitInitConfirmation);
  }

  function awaitInitConfirmation(e: MessageEvent) {
    let data = e.data;
    if (data === "ready") {
      queryWorker.postMessage(
        { op: "channel", data: { port: channel.port1 } }, [channel.port1],
      );

      cleanUp();
      then();
    } else {
      throw new Error(`Unexpected message: ${data}`);
    }
  }
  queryWorker.onmessage = awaitInitConfirmation;
}

function enableUI(): void {
  queryWorker.onmessage = handleQueryMessage;
  queryWorker.onmessageerror = handleQueryError;

  runButton.disabled = false;
  runButton.onclick = runQuery;
  moreButton.onclick = nextResult;
}

setup(enableUI);

function runQuery(): void {
  resultsBox.textContent = "";

  const query = queryBox.value;
  let vars;
  if (varsBox.value === null) {
    vars = {};
  } else {
    try {
      vars = JSON.parse(varsBox.value);
    } catch (e) {
      resultsBox.textContent = `Invalid variables data: ${e}`;
      return;
    }
  }

  moreButton.disabled = false;

  queryWorker.postMessage({
    op: "query",
    query,
    args: vars,
  });
}

function nextResult(): void {
  queryWorker.postMessage({
    op: "next",
  });
}

function handleQueryMessage(e: MessageEvent): void {
  let outcome = e.data;
  if (outcome.done) {
    if (!resultsBox.textContent?.endsWith("***")) {
      resultsBox.textContent += "*** no more data ***";
      moreButton.disabled = true;
    }
  } else {
    let pretty = JSON.stringify(outcome.value, null, 2);
    resultsBox.textContent += `${pretty}\n`;
  }
  resultsBox.scrollTop = resultsBox.scrollHeight;
  resultsBox.scrollIntoView();
}

function handleQueryError(e: MessageEvent): void {
  moreButton.disabled = true;
  resultsBox.textContent = `Query error: ${e.data}`;
}
