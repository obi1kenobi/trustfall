# trustfall_stubgen

Given a Trustfall schema, autogenerate a high-quality Rust adapter stub
fully wired up with all types, properties, and edges referenced in the schema.

First, install the CLI with: `cargo install --locked trustfall_stubgen`
Then generate Trustfall adapter stubs for your schema with:
```
trustfall_stubgen --schema <your_schema.graphql> --target <output_directory>
```
Under the hood this directly calls the [`generate_rust_stub()`](https://docs.rs/trustfall_stubgen/latest/trustfall_stubgen/fn.generate_rust_stub.html) function from this crate.
This crate can also be used as a library, so you can call that function directly from
your own code without going through the CLI.

The generated Trustfall adapter stub has the following structure:
| file name              | purpose                                                |
| ---------------------- | ------------------------------------------------------ |
| adapter/mod.rs         | connects everything together                           |
| adapter/schema.graphql | contains the schema for the adapter                    |
| adapter/adapter.rs     | contains the adapter implementation                    |
| adapter/vertex.rs      | contains the vertex type definition                    |
| adapter/entrypoints.rs | contains the entry points where all queries must start |
| adapter/properties.rs  | contains the property implementations                  |
| adapter/edges.rs       | contains the edge implementations                      |
| adapter/tests.rs       | contains test code                                     |

See an example of
[a generated adapter stub](https://github.com/obi1kenobi/trustfall/tree/main/trustfall_stubgen/test_data/expected_outputs/hackernews/adapter)
from this crate's test suite.
