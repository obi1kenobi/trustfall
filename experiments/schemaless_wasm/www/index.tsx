import 'core-js/stable';
import * as React from 'react';
import { render } from 'react-dom';

import App from './App';

const bodyEl = document.body;
render(<App />, bodyEl);
if (process.env.NODE_ENV === 'development' && module.hot) {
    module.hot.accept('./App', () => render(<App />, bodyEl));
}
