# Data Widgets

A truly-serverless querying demo: query HackerNews and rustdoc entirely from your browser. No backend servers or lambdas involved!

Your browser fetches data directly from HackerNews and executes queries using `trustfall_wasm`. For rustdoc, your browser downloads the rustdoc JSON file and queries it in-memory using `trustfall_wasm`.

See the `example_queries` directory for some example queries to run. Live demo here: https://predr.ag/demo/trustfall/

## Development

- Install the version of node with a version >= 16.13 and `npm`.
- Since node v16.13, node ships corepack for managing other package managers, and we use pnpm, so to enable corepack run:

```bash
corepack enable
```

If you installed Node.js using Homebrew, you'll need to install corepack separately:

```bash
brew install corepack
```

-  In order to install dependencies:
```bash
pnpm install # install dependencies, similar to `npm install`, but we use `pnpm` to apply our patches too
```
- `pnpm run build:trustfall` to build the `trustfall_wasm` crate to WASM, saved in the git-ignored `www2` directory.
- `pnpm run build:rustdoc` to build the `trustfall_rustdoc` crate to WASM
- `pnpm start` to build and run the local server
- Open a browser to `http://localhost:8000/hackernews` or `http://localhost:8000/rustdoc` and enjoy!
