from typing import Any, Mapping, Iterable, Iterator, Tuple

from .. import Adapter, Context


_NUMBER_NAMES = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
]

Vertex = int


class NumbersAdapter(Adapter[Vertex]):
    def resolve_starting_vertices(
        self,
        edge_name: str,
        parameters: Mapping[str, Any],
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Vertex]:
        max_value = parameters["max"]

        # We could just `yield from range(0, max_value)` here.
        # But that returns an `Iterator` which is an easier type to deal with than `Iterable`,
        # and we've had a bug with correct `Iterable` handling already.
        # So let's return an `Iterable` instead by wrapping it in `list()`.
        return list(range(0, max_value))

    def resolve_property(
        self,
        contexts: Iterator[Context[Vertex]],
        type_name: str,
        property_name: str,
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[Context[Vertex], Any]]:
        for context in contexts:
            active_vertex = context.active_vertex
            value = None
            if active_vertex is not None:
                if property_name == "value":
                    value = active_vertex
                elif property_name == "name":
                    if 0 <= active_vertex < len(_NUMBER_NAMES):
                        value = _NUMBER_NAMES[active_vertex]
                else:
                    raise NotImplementedError()

            yield (context, value)

    def resolve_neighbors(
        self,
        contexts: Iterator[Context[Vertex]],
        type_name: str,
        edge_name: str,
        parameters: Mapping[str, Any],
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[Context[Vertex], Iterable[Vertex]]]:
        for context in contexts:
            active_vertex = context.active_vertex
            neighbors = []
            if active_vertex is not None:
                if edge_name == "multiple":
                    if active_vertex > 0:
                        max_value = parameters["max"]
                        neighbors = range(
                            2 * active_vertex,
                            max_value * active_vertex + 1,
                            active_vertex,
                        )
                elif edge_name == "predecessor":
                    if active_vertex > 0:
                        neighbors = [active_vertex - 1]
                elif edge_name == "successor":
                    neighbors = [active_vertex + 1]
                else:
                    raise NotImplementedError()

            yield (context, neighbors)

    def resolve_coercion(
        self,
        contexts: Iterator[Context[Vertex]],
        type_name: str,
        coerce_to_type: str,
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[Context[Vertex], bool]]:
        raise NotImplementedError()
