const HTMLPlugin = require('html-webpack-plugin');
const path = require('path');

module.exports = (env) => {
  const isDev = env !== 'production';
  const mode = isDev ? 'development' : 'production';
  return {
    entry: {
      index: './src/index.tsx',
      adapter: './src/hackernews/adapter.js',
      fetcher: './src/hackernews/fetcher.js',
    },
    output: {
      path: path.resolve(__dirname, 'dist'),
      filename: '[name].js',
    },
    devtool: isDev ? 'cheap-module-source-map' : 'source-map',
    mode,
    resolve: {
      extensions: ['.tsx', '.js', '.js', '.json'],
    },
    experiments: {
      asyncWebAssembly: true,
      topLevelAwait: true,
    },
    plugins: [
      new HTMLPlugin({
        title: 'Trustfall Query Playground',
        chunks: ['index'],
        meta: {
          viewport: 'minimum-scale=1, initial-scale=1, width=device-width',
        },
        template: './src/index.ejs',
      }),
    ],
    module: {
      rules: [
        {
          test: /\.(js|ts|tsx)$/,
          use: {
            loader: 'babel-loader',
            options: { cacheDirectory: true, envName: mode },
          },
          exclude: /node_modules/,
        },
        {
          test: /\.(eot|otf|svg|ttf|woff|woff2|gif)$/,
          type: 'asset/resource',
        },
        {
          test: /\.example$/,
          type: 'asset/source',
        },
        {
          test: /\.graphql$/,
          type: 'asset/source',
        },
        {
          test: /\.css$/i,
          use: ['style-loader', 'css-loader'],
        },
      ],
    },
    devServer: {
      historyApiFallback: true,
      host: 'localhost',
      hot: true,
      open: true,
      port: 8000,
      // Headers necessary for SharedArrayBuffer usage
      headers: {
        'Cross-Origin-Opener-Policy': 'same-origin',
        'Cross-Origin-Embedder-Policy': 'require-corp',
      },
      devMiddleware: {
        stats: 'minimal',
      },
    },
  };
};
