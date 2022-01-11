from typing import Any, Mapping, Iterable, Iterator, Tuple

from .. import Adapter, DataContext


_NUMBER_NAMES = [
    "zero", "one", "two", "three", "four",
    "five", "six", "seven", "eight", "nine", "ten",
]

Token = int

class NumbersAdapter(Adapter[Token]):
    def get_starting_tokens(
        self,
        edge_name: str,
        parameters: Mapping[str, Any],
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Token]:
        max_value = parameters["max"]
        yield from range(0, max_value)

    def project_property(
        self,
        data_contexts: Iterator[DataContext[Token]],
        type_name: str,
        field_name: str,
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[DataContext[Token], Any]]:
        for context in data_contexts:
            token = context.current_token
            value = None
            if token is not None:
                if field_name == "value":
                    value = token
                elif field_name == "name":
                    if 0 <= token < len(_NUMBER_NAMES):
                        value = _NUMBER_NAMES[token]
                else:
                    raise NotImplementedError()

            yield (context, value)

    def project_neighbors(
        self,
        data_contexts: Iterator[DataContext[Token]],
        type_name: str,
        edge_name: str,
        parameters: Mapping[str, Any],
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[DataContext[Token], Iterable[Token]]]:
        for context in data_contexts:
            token = context.current_token
            neighbors = []
            if token is not None:
                if edge_name == "multiple":
                    if token > 0:
                        max_value = parameters["max"]
                        neighbors = range(2 * token, max_value * token + 1, token)
                elif edge_name == "predecessor":
                    if token > 0:
                        neighbors = [token - 1]
                elif edge_name == "successor":
                    neighbors = [token + 1]
                else:
                    raise NotImplementedError()

            yield (context, neighbors)

    def can_coerce_to_type(
        self,
        data_contexts: Iterator[DataContext[Token]],
        type_name: str,
        coerce_to_type: str,
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[DataContext[Token], bool]]:
        raise NotImplementedError()
