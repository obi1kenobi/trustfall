export class Webpage {
  protected fetchPort: MessagePort;
  m_url: string;

  constructor(fetchPort: MessagePort, url: string) {
    this.fetchPort = fetchPort;
    this.m_url = url;
  }

  //   """
  //   The URL of the web page.
  //   """
  //   url: String!
  url(): string {
    return this.m_url;
  }

  get __typename(): string {
    return this.constructor.name;
  }
}
