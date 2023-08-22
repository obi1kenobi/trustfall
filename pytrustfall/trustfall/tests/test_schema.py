import unittest

from ..trustfall import (
    InvalidSchemaError,
    Schema,
)


INVALID_SCHEMA = """\
schema {
    query: RootSchemaQuery
}
directive @filter(op: String!, value: [String!]) repeatable on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD

type RootSchemaQuery {
    Base: Base
    Derived: Derived
}

interface Base {
    foo: Int!
    bar: String
}

type Derived implements Base {
    # `foo` is illegally widened by making it nullable,
    # and `bar` is missing entirely. Both of these are errors.
    foo: Int
}
"""


class SchemaTests(unittest.TestCase):
    def test_invalid_schema_raises_exception(self) -> None:
        self.assertRaises(InvalidSchemaError, Schema, INVALID_SCHEMA)
