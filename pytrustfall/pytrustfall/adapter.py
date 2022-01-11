from abc import ABCMeta, abstractmethod
from typing import Any, Generic, Iterable, Iterator, Mapping, Optional, Tuple, TypeVar


Token = TypeVar("Token")


class DataContext(Generic[Token]):
    __slots__ = ("current_token",)

    def __init__(self, current_token: Optional[Token]) -> None:
        self.current_token = current_token


class Adapter(Generic[Token], metaclass=ABCMeta):
    @abstractmethod
    def get_starting_tokens(
        self,
        edge_name: str,
        parameters: Mapping[str, Any],
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Token]:
        pass

    @abstractmethod
    def project_property(
        self,
        data_contexts: Iterator[DataContext[Token]],
        type_name: str,
        field_name: str,
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[DataContext[Token], Any]]:
        pass

    @abstractmethod
    def project_neighbors(
        self,
        data_contexts: Iterator[DataContext[Token]],
        type_name: str,
        edge_name: str,
        parameters: Mapping[str, Any],
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[DataContext[Token], Iterable[Token]]]:
        pass

    @abstractmethod
    def can_coerce_to_type(
        self,
        data_contexts: Iterator[DataContext[Token]],
        type_name: str,
        coerce_to_type: str,
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[DataContext[Token], bool]]:
        pass
