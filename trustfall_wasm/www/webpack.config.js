const path = require('path');
const CopyWebpackPlugin = require("copy-webpack-plugin");
const WasmPackPlugin = require('@wasm-tool/wasm-pack-plugin')

module.exports = {
  entry: "./bootstrap.js",
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "bootstrap.js",
    hashFunction: "xxhash64",
  },
  experiments: {
    syncWebAssembly: true,
  },
  mode: "development",
  plugins: [
    new CopyWebpackPlugin({
      patterns: ['index.html'],
    })
  ],
};
