# `trustfall_wasm` — the WebAssembly version of the `trustfall` engine

Embed state-of-the-art query capabilities directly into the browser,
enabling a new generation of local-first serverless applications.
Any query a server or serverless hosted function could run can also run
at least as well in your own browser.

## Browser demo of `trustfall_wasm`

See the `www` [directory](https://github.com/obi1kenobi/trustfall/tree/main/trustfall_wasm/www)
for an end-to-end demo of querying with `trustfall_wasm`,
including an adapter implemented in JavaScript and a schema instantiated from JavaScript.

## Building the `trustfall_wasm` module

Prerequisites:
- a recent Rust environment: [installation instructions](https://www.rust-lang.org/tools/install)
- the `wasm-pack` tool: [installation instructions](https://drager.github.io/wasm-pack/installer/)

All following commands are run from the `trustfall_wasm` directory.

The WASM module currently only supports web browser use, and does not support running in Node.
To run browser tests, make sure you have Firefox or Chrome installed, and then run
one of the following commands matching the browser you'd like to test in:
```
wasm-pack test --headless --firefox
wasm-pack test --headless --chrome
```

The TypeScript definitions file is currently hand-written, since
the `wasm-bindgen` auto-generated definitions are not as detailed
(e.g. wouldn't make the `Adapter` type appropriately generic).
Building the WASM module is therefore a two-step process:
- Build the WASM/JS files.
- Copy the hand-written TypeScript definitions into the build directory (by default, `pkg`).

To create a dev build, run the following commands:
```
wasm-pack build --dev
cp src/trustfall_wasm.d.ts pkg/
```

To create a release build, run the following commands:
```
wasm-pack build
cp src/trustfall_wasm.d.ts pkg/
```

At the end of either of these scripts, the build directory (by default, `pkg`) will contain
the built WASM module and all its supporting files.
