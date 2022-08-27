import { Suspense, lazy } from 'react';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { CircularProgress } from '@mui/material';

const HackerNewsPlayground = lazy(() => import('./playgrounds/HackerNews'));
const RustdocPlayground = lazy(() => import('./playgrounds/Rustdoc'));

export default function App() {
  return (
    <BrowserRouter>
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
    </BrowserRouter>
  );
}
