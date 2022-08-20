# trustfall_rustdoc

WASM implementation of querying rustdoc JSON output.

To build it for a web target, run the following from the `trustfall_rustdoc` directory:
```console
wasm-pack build --no-typescript --target web
cp ./src/trustfall_rustdoc.d.ts ./pkg/
```

To run the test suite in a headless Firefox browser, run:
```console
wasm-pack test --headless --firefox
```
