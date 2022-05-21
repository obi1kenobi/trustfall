const CopyWebpackPlugin = require("copy-webpack-plugin");
const path = require('path');

module.exports = {
  entry: "./bootstrap.js",
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "bootstrap.js",
  },
  mode: "development",
  plugins: [
    new HTMLPlugin({
        title: 'Query-based Schema Generator',
        meta: {
          viewport: 'minimum-scale=1, initial-scale=1, width=device-width',
        },
        template: './index.ejs',
      })
  ],
};
