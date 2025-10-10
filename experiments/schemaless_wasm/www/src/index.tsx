import 'core-js/stable';
import * as React from 'react';
import { createRoot } from 'react-dom/client';

import App from './App';

const IGNORABLE_RESIZE_OBSERVER_ERRORS = new Set([
    'ResizeObserver loop limit exceeded',
    'ResizeObserver loop completed with undelivered notifications.',
]);

if (typeof window !== 'undefined') {
    window.addEventListener(
        'error',
        (event) => {
            if (IGNORABLE_RESIZE_OBSERVER_ERRORS.has(event.message)) {
                event.stopImmediatePropagation();
            }
        },
        { capture: true }
    );
}

const mainEl = document.querySelector('main');

if (!mainEl) {
    throw new Error('Target <main> element not found in document.');
}

const root = createRoot(mainEl);

const renderApp = () => {
    root.render(<App />);
};

renderApp();

if (process.env.NODE_ENV === 'development' && module.hot) {
    module.hot.accept('./App', () => {
        renderApp();
    });
}
