from typing import Any, Dict, Iterator, Mapping, Type

from .adapter import Adapter
from .trustfall import AdapterShim, Schema, interpret_query


def execute_query(
    adapter: Adapter,
    schema: Schema,
    query: str,
    arguments: Mapping[str, Any],
) -> Iterator[Dict[str, Any]]:
    """Execute the given query using the adapter, returning an iterator of result dicts."""
    if not isinstance(adapter, Adapter):
        raise TypeError(
            f"Expected 'adapter' input to be a subclass of Adapter, but instead got: {adapter}"
        )

    return interpret_query(AdapterShim(adapter), schema, query, arguments)
