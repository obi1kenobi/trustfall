from typing import Any, Dict, Iterator, Mapping

from .adapter import Adapter, Vertex
from ._internals import AdapterShim, Schema, interpret_query


def execute_query(
    adapter: Adapter[Vertex],
    schema: Schema,
    query: str,
    arguments: Mapping[str, Any],
    /,
) -> Iterator[Dict[str, Any]]:
    """Execute the given query using the adapter, returning an iterator of result dicts."""
    if not isinstance(adapter, Adapter):
        raise TypeError(
            f"Expected 'adapter' input to be a subclass of Adapter, but instead got: {adapter}"
        )

    return interpret_query(AdapterShim(adapter), schema, query, arguments)
