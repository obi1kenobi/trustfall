export type RustdocWorkerMessage =
  | {
      op: 'query';
      query: string;
      vars: object;
    }
  | {
      op: 'load-crate';
      name: string;
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
    };
