from os import path
from textwrap import dedent
from typing import (
    Any,
    Callable,
    Dict,
    Iterable,
    Iterator,
    Mapping,
    Optional,
    Tuple,
    cast,
)
import unittest

from .. import (
    Context,
    FrontendError,
    ParseError,
    QueryArgumentsError,
    Schema,
    ValidationError,
    FieldValue,
)
from ..execution import execute_query
from .numbers_adapter import NumbersAdapter


def _get_numbers_schema() -> Schema:
    package_root = path.abspath(path.dirname(path.dirname(path.dirname(__file__))))
    schema_path = path.join(package_root, "numbers.graphql")
    with open(schema_path, "r") as f:
        return Schema(f.read())


SCHEMA = _get_numbers_schema()


class ExecutionTests(unittest.TestCase):
    def test_simple_query(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 10) {
                    value @output
                    name @output
                }
            }
            """
        )
        args: Dict[str, Any] = {}

        expected_result = [
            {"name": "zero", "value": 0},
            {"name": "one", "value": 1},
            {"name": "two", "value": 2},
            {"name": "three", "value": 3},
            {"name": "four", "value": 4},
            {"name": "five", "value": 5},
            {"name": "six", "value": 6},
            {"name": "seven", "value": 7},
            {"name": "eight", "value": 8},
            {"name": "nine", "value": 9},
        ]
        actual_result = list(execute_query(NumbersAdapter(), SCHEMA, query, args))
        self.assertEqual(expected_result, actual_result)

    def test_query_with_list_typed_input(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 10) {
                    value @output @filter(op: "one_of", value: ["$numbers"])
                    name @output
                }
            }
            """
        )
        args: Dict[str, Any] = {
            "numbers": [1, 3, 4, 5],
        }

        expected_result = [
            {"name": "one", "value": 1},
            {"name": "three", "value": 3},
            {"name": "four", "value": 4},
            {"name": "five", "value": 5},
        ]
        actual_result = list(execute_query(NumbersAdapter(), SCHEMA, query, args))
        self.assertEqual(expected_result, actual_result)

    def test_query_with_heterogeneous_list_argument(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 10) {
                    value @output @filter(op: "one_of", value: ["$numbers"])
                }
            }
            """
        )
        args: Dict[str, Any] = {
            "numbers": [1, 2.0, 3],
        }

        with self.assertRaisesRegex(
            ValueError,
            "Found elements of different \\(non\\-null\\) types in the same list",
        ):
            _ = list(execute_query(NumbersAdapter(), SCHEMA, query, args))

    def test_nested_query(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 4) {
                    value @output

                    multiple(max: 3) {
                        mul: value @output
                    }
                }
            }
            """
        )
        args: Dict[str, Any] = {}

        expected_result = [
            {"value": 1, "mul": 2},
            {"value": 1, "mul": 3},
            {"value": 2, "mul": 4},
            {"value": 2, "mul": 6},
            {"value": 3, "mul": 6},
            {"value": 3, "mul": 9},
        ]
        actual_result = list(execute_query(NumbersAdapter(), SCHEMA, query, args))
        self.assertEqual(expected_result, actual_result)

    def test_parse_error(self) -> None:
        query = "this isn't valid syntax"
        args: Dict[str, Any] = {}

        self.assertRaises(ParseError, execute_query, NumbersAdapter(), SCHEMA, query, args)

    def test_validation_error(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 4) {
                    nonexistent @output
                }
            }
            """
        )
        args: Dict[str, Any] = {}

        self.assertRaises(ValidationError, execute_query, NumbersAdapter(), SCHEMA, query, args)

    def test_frontend_error(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 4) {
                    value @output
                    value @output
                }
            }
            """
        )
        args: Dict[str, Any] = {}

        self.assertRaises(FrontendError, execute_query, NumbersAdapter(), SCHEMA, query, args)

    def test_query_arguments_error(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 4) {
                    value @output @filter(op: ">", value: ["$required"])
                }
            }
            """
        )
        args: Dict[str, Any] = {
            "not_used": 42,
        }

        self.assertRaises(QueryArgumentsError, execute_query, NumbersAdapter(), SCHEMA, query, args)

    def test_wrong_argument_type_error(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 4) {
                    value @output @filter(op: ">", value: ["$num"])
                }
            }
            """
        )
        args: Dict[str, Any] = {
            "num": "text instead of a number",
        }

        self.assertRaises(QueryArgumentsError, execute_query, NumbersAdapter(), SCHEMA, query, args)

    def test_none_value_for_non_nullable_argument_error(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 4) {
                    value @output @filter(op: "=", value: ["$num"])
                }
            }
            """
        )
        args: Dict[str, Any] = {
            "num": None,
        }

        self.assertRaises(QueryArgumentsError, execute_query, NumbersAdapter(), SCHEMA, query, args)

    def test_unrepresentable_field_value(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 4) {
                    value @output @filter(op: ">", value: ["$required"])
                }
            }
            """
        )
        args: Dict[str, Any] = {
            "required": object(),
        }

        self.assertRaises(
            ValueError, execute_query, NumbersAdapter(), SCHEMA, query, args
        )

    def test_bad_query_input_type(self) -> None:
        query = 123
        args: Dict[str, Any] = {}

        self.assertRaises(TypeError, execute_query, NumbersAdapter(), SCHEMA, query, args)

    def test_bad_args_input_type(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 4) {
                    value @output
                }
            }
            """
        )
        args = 123

        self.assertRaises(TypeError, execute_query, NumbersAdapter(), SCHEMA, query, args)

    def test_bad_schema_input_type(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 4) {
                    value @output
                }
            }
            """
        )
        args: Dict[str, Any] = {}

        self.assertRaises(TypeError, execute_query, NumbersAdapter(), 123, query, args)

    def test_bad_adapter_input_type(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 4) {
                    value @output
                }
            }
            """
        )
        args: Dict[str, Any] = {}

        self.assertRaises(TypeError, execute_query, 123, SCHEMA, query, args)


class OverridableAdapter(NumbersAdapter):
    starting_fn: Mapping[str, Callable[[Mapping[str, FieldValue]], Iterable[Any]]]
    property_fn: Mapping[
        Tuple[str, str],
        Callable[[Iterable[Context[Any]]], Iterable[Tuple[Context[Any], FieldValue]]],
    ]
    neighbor_fn: Mapping[
        Tuple[str, str],
        Callable[
            [Iterable[Context[Any]], Mapping[str, FieldValue]],
            Iterable[Tuple[Context[Any], Iterable[Any]]],
        ],
    ]
    coercion_fn: Mapping[
        str,
        Callable[[Iterable[Context[Any]], str], Iterable[Tuple[Context[Any], bool]]],
    ]

    def __init__(
        self,
        *,
        starting_fn: Optional[
            Mapping[str, Callable[[Mapping[str, FieldValue]], Iterable[Any]]]
        ] = None,
        property_fn: Optional[
            Mapping[
                Tuple[str, str],
                Callable[
                    [Iterable[Context[Any]]],
                    Iterable[Tuple[Context[Any], FieldValue]],
                ],
            ]
        ] = None,
        neighbor_fn: Optional[
            Mapping[
                Tuple[str, str],
                Callable[
                    [Iterable[Context[Any]], Mapping[str, FieldValue]],
                    Iterable[Tuple[Context[Any], Iterable[Any]]],
                ],
            ]
        ] = None,
        coercion_fn: Optional[
            Mapping[
                str,
                Callable[
                    [Iterable[Context[Any]], str], Iterable[Tuple[Context[Any], bool]]
                ],
            ]
        ] = None,
    ) -> None:
        self.starting_fn = starting_fn or dict()
        self.property_fn = property_fn or dict()
        self.neighbor_fn = neighbor_fn or dict()
        self.coercion_fn = coercion_fn or dict()

    def resolve_starting_vertices(
        self,
        edge_name: str,
        parameters: Mapping[str, Any],
        /,
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Any]:
        if (resolver := self.starting_fn.get(edge_name)) is not None:
            yield from resolver(parameters)
        else:
            yield from super().resolve_starting_vertices(
                edge_name, parameters, *args, **kwargs
            )

    def resolve_property(
        self,
        contexts: Iterator[Context[Any]],
        type_name: str,
        property_name: str,
        /,
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[Context[Any], FieldValue]]:
        if (resolver := self.property_fn.get((type_name, property_name))) is not None:
            yield from resolver(contexts)
        else:
            yield from super().resolve_property(
                contexts, type_name, property_name, *args, **kwargs
            )

    def resolve_neighbors(
        self,
        contexts: Iterator[Context[Any]],
        type_name: str,
        edge_name: str,
        parameters: Mapping[str, Any],
        /,
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[Context[Any], Iterable[Any]]]:
        if (resolver := self.neighbor_fn.get((type_name, edge_name))) is not None:
            yield from resolver(contexts, parameters)
        else:
            yield from super().resolve_neighbors(
                contexts, type_name, edge_name, parameters, *args, **kwargs
            )

    def resolve_coercion(
        self,
        contexts: Iterator[Context[Any]],
        type_name: str,
        coerce_to_type: str,
        /,
        *args: Any,
        **kwargs: Any,
    ) -> Iterable[Tuple[Context[Any], bool]]:
        if (resolver := self.coercion_fn.get(type_name)) is not None:
            yield from resolver(contexts, coerce_to_type)
        else:
            yield from super().resolve_coercion(
                contexts, type_name, coerce_to_type, *args, **kwargs
            )


class BadAdapterTests(unittest.TestCase):
    def test_ensure_overridable_adapter_works(self) -> None:
        query = dedent(
            """\
            {
                Number(max: 10) {
                    value @output
                    name @output
                }
            }
            """
        )
        args: Dict[str, Any] = {}

        expected_result = [
            {"name": "zero", "value": 0},
            {"name": "one", "value": 1},
            {"name": "two", "value": 2},
            {"name": "three", "value": 3},
            {"name": "four", "value": 4},
            {"name": "five", "value": 5},
            {"name": "six", "value": 6},
            {"name": "seven", "value": 7},
            {"name": "eight", "value": 8},
            {"name": "nine", "value": 9},
        ]
        actual_result = list(execute_query(OverridableAdapter(), SCHEMA, query, args))
        self.assertEqual(expected_result, actual_result)

    def test_invalid_property_value_resolved(self) -> None:
        def value_fn(
            contexts: Iterable[Context[Any]],
        ) -> Iterable[Tuple[Context[Any], FieldValue]]:
            # Don't mind this invalid cast.
            # We're explicitly testing that this error is caught at runtime.
            value = cast(FieldValue, object())
            for ctx in contexts:
                yield ctx, value

        property_fn = {
            ("Number", "value"): value_fn,
        }
        adapter = OverridableAdapter(property_fn=property_fn)

        query = dedent(
            """\
            {
                Number(max: 10) {
                    value @output
                    name @output
                }
            }
            """
        )
        args: Dict[str, Any] = {}

        # TODO: in an ideal world, this wouldn't be a `PanicException`
        #       and instead would be a more common exception type
        with self.assertRaisesRegex(
            BaseException,
            "resolve_property\\(\\) tuple element at index 1 is not a property value",
        ):
            _ = list(execute_query(adapter, SCHEMA, query, args))

    def test_invalid_neighbor_resolved(self) -> None:
        def successor_fn(
            contexts: Iterable[Context[Any]],
            parameters: Mapping[str, FieldValue],
        ) -> Iterable[Tuple[Context[Any], Iterator[Any]]]:
            for ctx in contexts:
                # Don't mind this invalid cast.
                # We're explicitly testing that this error is caught at runtime.
                neighbors = cast(Iterator[Any], object())
                yield ctx, neighbors

        neighbor_fn = {
            ("Number", "successor"): successor_fn,
        }
        adapter = OverridableAdapter(neighbor_fn=neighbor_fn)

        query = dedent(
            """\
            {
                Number(max: 2) {
                    successor {
                        value @output
                    }
                }
            }
            """
        )
        args: Dict[str, Any] = {}

        # TODO: in an ideal world, this wouldn't be a `PanicException`
        #       and instead would be a more common exception type
        with self.assertRaisesRegex(
            BaseException,
            "resolve_neighbors\\(\\) yielded tuple's second element is not an iterable",
        ):
            _ = list(execute_query(adapter, SCHEMA, query, args))
