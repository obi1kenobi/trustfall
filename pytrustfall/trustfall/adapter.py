from abc import ABCMeta, abstractmethod
from typing import Any, Generic, Iterable, Iterator, Mapping, Optional, Tuple, TypeVar


"""
The type of vertices in the dataset implemented by an adapter.
"""
Vertex = TypeVar("Vertex")


class Context(Generic[Vertex]):
    """Adapter helper type. Its active_vertex property indicates vertices whose data is needed."""

    __slots__ = ("_active_vertex",)

    @property
    def active_vertex(self) -> Optional[Vertex]:
        """The vertex whose information (properties, edges, etc.) needs to be resolved."""
        return self._active_vertex


class Adapter(Generic[Vertex], metaclass=ABCMeta):
    @abstractmethod
    def resolve_starting_vertices(
        self,
        edge_name: str,
        parameters: Mapping[str, Any],
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Vertex]:
        """Produce an iterable of vertices for the specified starting edge.

        Starting edges are ones where queries are allowed to begin.
        They are defined directly on the root query type of the schema.
        For example, `Foo` is the starting edge of the following query:
        ```graphql
        query {
            Foo {
                bar @output
            }
        }
        ```

        The caller guarantees that:
        - The specified edge is a starting edge in the schema being queried.
        - Any parameters the edge requires per the schema have values provided.
        """

    @abstractmethod
    def resolve_property(
        self,
        contexts: Iterator[Context[Vertex]],
        type_name: str,
        property_name: str,
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[Context[Vertex], Any]]:
        """Resolve the value of a vertex property, over an iterator of query contexts.

        Each context in the `contexts` argument has an active vertex of type Optional[Vertex],
        which is either None or represents a vertex of type `type_name` defined in the schema.

        This function resolves the property value on that active vertex.

        The caller guarantees that:
        - `type_name` is a type or interface defined in the schema.
        - `property_name` is either a property field on `type_name` defined in the schema,
          or the special value `"__typename"` requesting the name of the vertex's type.
        - When the active vertex is not None, then it's a vertex of type `type_name`:
          either its type is exactly `type_name`, or `type_name` is an interface that
          the vertex's type implements.

        The returned iterable must satisfy these properties:
        - Produce `(context, property_value)` tuples with the property's value for that context.
        - Produce contexts in the same order as the input `contexts` iterator produced them.
        - Produce property values whose type matches the property's type defined in the schema.
        - When a context's active vertex is `None`, its property value is `None`.
        """

    @abstractmethod
    def resolve_neighbors(
        self,
        contexts: Iterator[Context[Vertex]],
        type_name: str,
        edge_name: str,
        parameters: Mapping[str, Any],
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[Context[Vertex], Iterable[Vertex]]]:
        """Resolve the neighboring vertices across an edge, over an iterator of query contexts.

        Each context in the `contexts` argument has an active vertex of type Optional[Vertex],
        which is either None or represents a vertex of type `type_name` defined in the schema.

        This function resolves the neighboring vertices for that active vertex.

        If the schema this adapter covers has no edges aside from starting edges, then
        this method will never be called and may be implemented as `raise NotImplementedError()`.

        The caller guarantees that:
        - `type_name` is a type or interface defined in the schema.
        - `edge_name` is an edge field on `type_name` defined in the schema.
        - Any parameters the edge requires per the schema have values provided.
        - When the active vertex is not None, then it's a vertex of type `type_name`:
          either its type is exactly `type_name`, or `type_name` is an interface that
          the vertex's type implements.

        The returned iterator must satisfy these properties:
        - Produce `(context, neighbors)` tuples with an iterator of neighbor vertices for that edge.
        - Produce contexts in the same order as the input `contexts` iterator produced them.
        - Each neighboring vertex is of the type specified for that edge in the schema.
        - When a context's active vertex is None, it has an empty neighbors iterator.
        """

    @abstractmethod
    def resolve_coercion(
        self,
        contexts: Iterator[Context[Vertex]],
        type_name: str,
        coerce_to_type: str,
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[Context[Vertex], bool]]:
        """Attempt to coerce vertices to a subtype, over an iterator of query contexts.

        In this example query, the starting vertices of type `Foo` are coerced to `Bar`:
        ```graphql
        query {
            Foo {
                ... on Bar {
                    abc @output
                }
            }
        }
        ```

        Each context in the `contexts` argument has an active vertex of type `Optional[Vertex]`,
        which is either None or represents a vertex of type `type_name` defined in the schema.

        This function checks whether the active vertex is of the specified subtype.

        If this adapter's schema contains no subtyping, then no type coercions are possible:
        this method will never be called and may be implemented as `raise NotImplementedError()`.

        The caller guarantees that:
        - `type_name` is an interface defined in the schema.
        - `coerce_to_type` is a type or interface that implements `type_name` in the schema.
        - When the active vertex is not None, then it's a vertex of type `type_name`:
          either its type is exactly `type_name`, or `type_name` is an interface that
          the vertex's type implements.

        The returned iterator must satisfy these properties:
        - Produce `(context, can_coerce)` tuples showing if the coercion succeded for that context.
        - Produce contexts in the same order as the input `contexts` iterator produced them.
        - Each neighboring vertex is of the type specified for that edge in the schema.
        - When a context's active vertex is `None`, its coercion outcome is `False`.
        """
