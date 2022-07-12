# Data Widgets

A truly-serverless querying demo: query HackerNews entirely from your browser. No backend servers or lambdas involved! Your browser fetches data directly from HackerNews and executes queries using `trustfall_wasm`.

Only tested to work in Chrome. Known to not work in Firefox since [Firefox does not support modules in web workers](https://bugzilla.mozilla.org/show_bug.cgi?id=1247687).

See the `example_queries` directory for some example queries to run. Live demo here: https://predr.ag/demo/trustfall/

Contents:

- `src` contains the TypeScript source code for the demo.
- Those source files are built as JavaScript (currently, without a bundler) into the `www` directory (not checked in).
- The `www2` directory contains the `trustfall_wasm` WASM bindings for the `trustfall` query engine. In principle, this shouldn't be checked in, but is checked in for convenience since this is a custom bundler-less build (unlike the regular version which assumes use with a bundler).
- `server.py` is a simple Python web server that can be used to serve all the needed files with the proper (security-related) headers required to make things work. It should work with any reasonably-modern Python 3 and has no dependencies: just run `python -m server`.

## Development

- Install a new-ish version of Node and `npx`.
- `npm install --include=dev` to install the TypeScript compiler.
- `npx tsc -p tsconfig.json` to compile the TypeScript to JavaScript.
- `python -m server` from the root of the project to start serving on port 8000.
- Open a browser to `http://localhost:8000/` and enjoy!
