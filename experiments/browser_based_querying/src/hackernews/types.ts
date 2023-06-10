export interface Hackernews {
  Firebase: {
    Item: {
      id: number;
      time: number;
      type: string;
    };

    Story: {
      id: number;
      time: number;
      by: string;
      score: number;
      text?: string;
      title: string;
      url?: string;
      type: string;
    };

    User: {
      id: number;
      karma: number;
      about?: string;
      created: number;
      submitted: number[];
    };

    Job: {
      id: number;
      time: number;
      type: string;
      title: string;
      score: number;
      url: string;
    };

    Comment: {
      id: number;
      time: number;
      type: string;
      by: string;
      text: string;
      parent?: number;
      kids?: number[];
      deleted?: true;
    };
  };
}
