export type RustdocWorkerMessage =
  | {
      op: 'query';
      query: string;
      vars: object;
    }
  | {
      op: 'load-crate';
      name: string;
      source: 'crates.io' | 'rustc';
    };

export type RustdocWorkerResponse =
  | {
      type: 'load-crate-ready';
      name: string;
    }
  | {
      type: 'load-crate-error';
      message: string;
    }
  | {
      type: 'query-ready';
      results: object[];
    }
  | {
      type: 'query-error';
      message: string;
    }
  | {
      type: 'ready';
    }
