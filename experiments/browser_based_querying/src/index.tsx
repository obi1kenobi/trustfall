import 'core-js/stable';
import { render } from 'react-dom';
import { createRoot } from 'react-dom/client';

import HackerNews from './playgrounds/HackerNews';

const mainEl = document.getElementsByTagName('main')[0];
const root = createRoot(mainEl);
root.render(<HackerNews />);
if (process.env.NODE_ENV === 'development' && module.hot) {
  module.hot.accept('./App', () => render(<HackerNews />, mainEl));
}
