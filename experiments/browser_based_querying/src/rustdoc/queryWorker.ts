import { CrateInfo, runQuery, makeCrateInfo } from '../../pkg/trustfall_rustdoc';
import { RustdocWorkerMessage, RustdocWorkerResponse } from './types';

function fetchCrateJson(filename: string, source: 'crates.io' | 'rustc'): Promise<string> {
  switch (source) {
    case 'crates.io':
      return fetch(
        `https://raw.githubusercontent.com/obi1kenobi/crates-rustdoc/main/rustdoc-v26/max-version/${filename}.json`
      ).then((response) => response.text());
    case 'rustc':
      return fetch(
        `https://raw.githubusercontent.com/obi1kenobi/crates-rustdoc/main/rustdoc-v26/rust-1.73.0-nightly/${filename}.json`
      ).then((response) => response.text());
  }
}

const send: (message: RustdocWorkerResponse) => void = postMessage;

let crateInfo: CrateInfo | null = null;

function dispatch(evt: MessageEvent<RustdocWorkerMessage>) {
  const msg = evt.data;

  switch (msg.op) {
    case 'query':
      if (crateInfo == null) {
        send({ type: 'query-error', message: 'No crate info loaded.' });
        return;
      }

      try {
        const results = runQuery(crateInfo, msg.query, msg.vars);
        send({ type: 'query-ready', results });
      } catch (message) {
        send({ type: 'query-error', message: message as string });
      }
      break;

    case 'load-crate':
      crateInfo = null;
      fetchCrateJson(msg.name, msg.source)
        .then((crateJson) => {
          try {
            crateInfo = makeCrateInfo(crateJson);
            send({ type: 'load-crate-ready', name: msg.name });
          } catch (e) {
            crateInfo = null;
            send({ type: 'load-crate-error', message: e as string });
          }
        })
        .catch(() => {
          send({
            type: 'load-crate-error',
            message: 'Something went wrong while fetching crate info.',
          });
        });
      break;
  }
}

onmessage = dispatch;
send({ type: 'ready' })
