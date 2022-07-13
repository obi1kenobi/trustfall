function run() {
  const queryWorker = new Worker('./adapter.js', { type: 'module' });
  const fetcherWorker = new Worker('./fetcher.js', { type: 'module' });
  const channel = new MessageChannel();

  queryWorker.postMessage({ op: 'init' });

  fetcherWorker.postMessage({ op: 'channel', data: { port: channel.port2 } }, [channel.port2]);

  function awaitInitConfirmation(e: MessageEvent) {
    const data = e.data;
    if (data === 'ready') {
      cleanUp();
      execute();
    } else {
      throw new Error(`Unexpected message: ${data}`);
    }
  }
  queryWorker.onmessage = awaitInitConfirmation;

  function cleanUp(): any {
    queryWorker.removeEventListener('message', awaitInitConfirmation);
  }

  function execute(): any {
    queryWorker.postMessage({ op: 'channel', data: { port: channel.port1 } }, [channel.port1]);

    queryWorker.onmessage = function (e: MessageEvent) {
      const data = e.data;
      console.log('Query msg received:', data);
    };

    queryWorker.postMessage({
      op: 'query',
      query: `
{
  HackerNewsTop(max: 30) {
    ... on HackerNewsStory {
      title @output
      byUsername @output
      url @output
      score @output @filter(op: ">=", value: ["$minScore"])

      byUser {
        karma @output @filter(op: ">=", value: ["$minKarma"])
      }
    }
  }
}
  `,
      args: {
        minScore: 25,
        minKarma: 200,
      },
    });

    queryWorker.postMessage({
      op: 'next',
    });

    queryWorker.postMessage({
      op: 'query',
      query: `
{
  HackerNewsLatestStories(max: 30) {
    title @output
    byUsername @output
    url @output
    score @output @filter(op: ">=", value: ["$minScore"])

    byUser {
      karma @output @filter(op: ">=", value: ["$minKarma"])
    }
  }
}
  `,
      args: {
        minScore: 3,
        minKarma: 200,
      },
    });

    queryWorker.postMessage({
      op: 'next',
    });

    queryWorker.postMessage({
      op: 'query',
      query: `
{
  HackerNewsUser(name: "patio11") {
    karma @output
    about @output

    submitted {
      ... on HackerNewsComment {
        text @output
      }
    }
  }
}
  `,
      args: {},
    });

    queryWorker.postMessage({
      op: 'query',
      query: `
{
  HackerNewsUser(name: "hopefullynonexistentsoicantestthis") {
    karma @output
    about @output
  }
}
  `,
      args: {},
    });
  }
}

run();
