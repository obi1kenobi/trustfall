from os import path
from textwrap import dedent
from typing import Any, Dict
import unittest

from ..trustfall import (
    FrontendError,
    InvalidIRQueryError,
    ParseError,
    QueryArgumentsError,
    Schema,
    ValidationError,
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

        self.assertRaises(QueryArgumentsError, execute_query, NumbersAdapter(), SCHEMA, query, args)

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
