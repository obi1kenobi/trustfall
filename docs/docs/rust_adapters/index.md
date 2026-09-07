# Rust Adapters

An adapter maps a Trustfall schema to data. Resolver output follows input context order; that is
how the engine joins properties, edges, filters, and folds without collecting a whole query.

## Infallible adapters

Start with `BasicAdapter`. It uses `&str` field names and resolves `__typename` automatically.
Implement `Adapter` directly when resolver metadata or `Arc<str>` names are useful.

```rust
impl<'vertex> BasicAdapter<'vertex> for Catalog {
    type Vertex = Vertex;

    // Implement the four resolver methods with ordinary values and iterators.
}
```

These traits are deliberately result-free: no error associated type, `Ok(...)` wrapping, or
`map(Ok)` boundary plumbing. `execute_query()` yields plain rows.

## Resolver failures

If the data source can fail while resolving a query, implement `FallibleAdapter` and call
`try_execute_query()` instead. It carries the adapter's concrete error at the resolver boundary:
starting vertices, property and coercion outcomes, and both levels of edge resolution.

Query parsing and argument validation fail before iteration. Resolver failures appear as
`ExecutionError<E>` rows in input order, so the caller owns the policy: surface the error, skip
that row, or stop at the first failure.

```rust
let rows = try_execute_query(&schema, adapter, query, variables)?;
for row in rows {
    match row {
        Ok(row) => consume(row),
        Err(error) => report(error),
    }
}
```

Do not use `FallibleAdapter` merely to model an infallible source. The direct traits make the
common case smaller and easier to read.

## Async adapters

Enable the `async` feature to use `AsyncBasicAdapter` or `AsyncAdapter`.

```toml
[dependencies]
trustfall = { version = "0.8", features = ["async"] }
```

`AsyncBasicAdapter` has the same direct shape as `BasicAdapter`, with `Stream` in place of
`Iterator`; it handles `__typename` and uses `&str` field names. Use `AsyncAdapter` when resolver
metadata or `Arc<str>` names are useful. Both return ordinary streams and
`execute_query_async()` yields plain rows.

For an async data source that can fail, implement `FallibleAsyncAdapter` and use
`try_execute_query_async()`. Its context and outcome streams carry `Result`, preserving earlier
errors at their original positions. An error that prevents resolving an edge belongs to the outer
context stream; one encountered while producing neighbors belongs to that edge's inner neighbor
stream. Both become `ExecutionError<E>` result rows.

The async API is runtime-independent and does not require `Send`. Drive its stream on the
executor that owns the adapter. For concurrent per-context work, use the ordered buffered helpers
in `trustfall::provider::async_helpers`.

## Resolver rules

- Return one outcome for every input context, in input order.
- A missing `@optional` vertex resolves to `Null`, no neighbors, or `false`.
- Keep edge neighbor iterators or streams lazy where the data source permits it.
- In fallible async adapters, pass upstream error items through unchanged.

The [Rust examples](https://github.com/obi1kenobi/trustfall/tree/main/trustfall/examples) show
complete `BasicAdapter` implementations.
