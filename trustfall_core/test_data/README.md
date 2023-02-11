# Testing `trustfall_core`

In addition to small unit tests for specific functionality, `trustfall_core` includes
snapshot-style tests across the different layers of the compiler toolchain.
They ensure that running a query, or parsing a schema, continues to produce the same
outcomes at all levels of the compiler even as it continues to evolve.

Contents:
- [Testing query behavior](#testing-query-behavior)
- [Testing schema parsing](#testing-schema-parsing)

## Testing query behavior

Query processing in `trustfall_core` includes these stages:
- syntax parsing
- conversion to IR ("frontend")
- execution via the Trustfall interpreter

Each of the stages may succeed in producing output (i.e. the input of the next stage, if any),
or fail with an error. We test both successes and failures: we compare the result of the stage
to the expected result (successful output or error).

The subdirectories here contain files named like `X.Y.ron`:
- The `X` is the test case name, describing what is being tested.
- The `Y` specifies which stage's input or output we're looking at.
- `.ron` is the [Rusty Object Notation](https://github.com/ron-rs/ron) extension.

Different subdirectories here will have different options for `Y`.
For example, in the `valid_queries` subdirectory:
- `Y = graphql` files contain a query to be used as input to the syntax parsing stage.
- `Y = graphql-parsed` files contain the parse tree: output from syntax parsing,
  input to the frontend.
- `Y = ir` files contain the [intermediate representation](https://en.wikipedia.org/wiki/Intermediate_representation) for the query: output from the frontend, input to the interpreter
- `Y = trace` files contain an execution trace showing the exact series of steps the interpreter
  took in order to produce its results, as well as the final results of running the query.

Other subdirectories test error conditions at various stages, and will have a subset
of these `Y` options as well as some new ones for stage errors:
`Y = parse-error`, `Y = frontend-error`, or `Y = exec-error`.

### Composition of tests ➡ End-to-end tests

Trustfall is a deterministic compiler; any non-determinism is considered a bug and
will be caught by tests. Therefore, per-stage tests are composable:
if each individual stage continues to produce the same output as before, we are guaranteed that
the compiler (as a mere composition of stages) continues to produce the same output as well.

As a result, we get the benefit of end-to-end tests at the cost of per-stage tests.
This is convenient! `cargo test` can run our per-stage tests in parallel, while getting
the correctness benefits of running the equivalent sequential end-to-end tests.

## Testing schema parsing

Schema parsing is simpler to test since there's only one stage: parsing the schema.
We adopt a similar approach: we have test schemas that are expected to either parse successfully
(`valid_schemas` directory) or produce a specified error (`schema_errors` directory).

The naming scheme for schema files and their corresponding expected errors is similar to the one for
queries: schemas are named like `X.graphql` while their errors are named `X.schema-error.ron`.
