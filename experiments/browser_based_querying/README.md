# Data Widgets

A truly-serverless querying demo: query HackerNews and rustdoc entirely from your browser. No backend servers or lambdas involved!

Your browser fetches data directly from HackerNews and executes queries using `trustfall_wasm`. For rustdoc, your browser downloads the rustdoc JSON file and queries it in-memory using `trustfall_wasm`.

See the `example_queries` directory for some example queries to run. Live demo here: https://predr.ag/demo/trustfall/

## Development

- Install the version of node and `npm`.
- `npm install` to install dependencies
- `npm run build:trustfall` to build the `trustfall_wasm` crate to WASM, saved in the git-ignored `www2` directory.
- `npm run build:rustdoc` to build the `trustfall_rustdoc` crate to WASM
- `npm start` to build and run the local server
- Open a browser to `http://localhost:8000/hackernews` or `http://localhost:8000/rustdoc` and enjoy!
