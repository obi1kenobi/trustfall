import { Suspense, lazy } from 'react';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { CircularProgress, CssBaseline } from '@mui/material';
import { ThemeProvider, createTheme } from '@mui/material/styles';
import { QueryParamProvider } from 'use-query-params';
import GithubCorner from 'react-github-corner';
import { QueryFragmentAdapter } from './QueryFragmentAdapter';

const HackerNewsPlayground = lazy(() => import('./hackernews/Playground'));
const RustdocPlayground = lazy(() => import('./rustdoc/Playground'));

export default function App() {
  const theme = createTheme({
    components: {
      MuiCssBaseline: {
        styleOverrides: {
          "html, body": {
            overflow: 'hidden', height: '100%',
          },
          main: {
            height: "100%",
            overflowY: "auto",
            position: "relative",
          },
        },
      },
    },
  });
  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <BrowserRouter>
        <QueryParamProvider adapter={QueryFragmentAdapter}>
          <Routes>
            <Route
              path="/hackernews"
              element={
                <Suspense fallback={<CircularProgress />}>
                  <HackerNewsPlayground />
                  <GithubCorner href="https://github.com/obi1kenobi/trustfall" bannerColor='#1976d2' />
                </Suspense>
              }
            />
            <Route
              path="/rustdoc"
              element={
                <Suspense fallback={<CircularProgress />}>
                  <RustdocPlayground />
                  <GithubCorner href="https://github.com/obi1kenobi/trustfall" bannerColor='#1976d2' />
                </Suspense>
              }
            />
          </Routes>
        </QueryParamProvider>
      </BrowserRouter>
    </ThemeProvider>
  );
}
