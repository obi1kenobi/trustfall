import { Suspense, lazy } from 'react';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { CircularProgress } from '@mui/material';
import { QueryParamProvider } from 'use-query-params';
import { QueryFragmentAdapter } from './QueryFragmentAdapter';

const HackerNewsPlayground = lazy(() => import('./hackernews/Playground'));
const RustdocPlayground = lazy(() => import('./rustdoc/Playground'));

export default function App() {
  return (
    <BrowserRouter>
      <QueryParamProvider adapter={QueryFragmentAdapter}>
        <Routes>
          <Route
            path="/hackernews"
            element={
              <Suspense fallback={<CircularProgress />}>
                <HackerNewsPlayground />
              </Suspense>
            }
          />
          <Route
            path="/rustdoc"
            element={
              <Suspense fallback={<CircularProgress />}>
                <RustdocPlayground />
              </Suspense>
            }
          />
        </Routes>
      </QueryParamProvider>
    </BrowserRouter>
  );
}
