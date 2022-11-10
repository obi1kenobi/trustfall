# `trustfall` — Python bindings

Use the Trustfall query engine from Python.

## API

Create a schema object:
```python
from trustfall import Schema

my_schema = Schema(
    """
    Your schema text here, written in GraphQL SDL.

    See an example schema here:
    https://github.com/obi1kenobi/trustfall/blob/main/pytrustfall/numbers.graphql
    """
)
```

Create an adapter with which to query the schema:
```python
from typing import Any, Dict

from trustfall import Adapter

# Choose the type that is used to represent data vertices:
Vertex = Dict[str, Any]

class MyAdapter(Adapter[Vertex]):
    # Implement the four abstract methods from Adapter.
    ...
```

Execute queries:
```python
from trustfall import execute_query

my_adapter = MyAdapter(...)

my_query = """
query {
    # your query here
}
"""
args = {
    # query arguments here
}

results_iterator = execute_query(
    my_adapter,
    my_schema,
    my_query,
    args,
)
for result in results_iterator:
    print(result)
```

## Installing `trustfall`

This package is a wrapper around the Trustfall query engine, which is written in Rust.

Wheels are available for Windows, macOS (both x86 and ARM), and Linux (`manylinux`),
for each supported version of CPython (3.9+).

This package should work on other platforms as well, in which case its Rust components
may need to be compiled from source as part of the installation process.

If you get errors while installing this package, please
[report your OS and architecture info in an issue](https://github.com/obi1kenobi/trustfall/issues/new).
