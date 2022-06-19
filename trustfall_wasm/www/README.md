# `trustfall_wasm_www` — browser-based demo of using `trustfall_wasm`

A demo web site using the `trustfall_wasm` package with webpack.
It initializes the WASM module, sets up a query adapter, runs a query,
and prints the results to `console.log()`.

See the schema and query adapter used in this demo in the included `index.js`
[file](https://github.com/obi1kenobi/trustfall/tree/main/trustfall_wasm/www/index.js).

## Building and running `trustfall_wasm_www`

Prerequisites:
- a recent Rust environment: [installation instructions](https://www.rust-lang.org/tools/install)
- the `wasm-pack` tool: [installation instructions](https://rustwasm.github.io/wasm-pack/installer/)
- Node v16+ or newer: [installation instructions](https://nodejs.org/en/download/)

First, build `trustfall_wasm` by following the steps in the `trustfall_wasm`
[README.md](https://github.com/obi1kenobi/trustfall/tree/main/trustfall_wasm/README.md).
When this process is complete, you should have a `trustfall_wasm/pkg/` directory in the repo
containing the `trustfall_wasm` WASM module and supporting files.

Then, run `npm install` to install the dependencies for building and running the web server.
Finally, run `npm start` to start the webpack dev server and open a browser tab with
the `trustfall_wasm_www` demo website.
