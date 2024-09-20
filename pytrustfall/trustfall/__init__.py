from .adapter import Adapter, Context, FieldValue
from .execution import execute_query

from ._internals import Schema

# Error types:
# - ParseError, when the provided input doesn't even parse as valid syntax
# - ValidationError, when the input is syntactically valid but doesn't match the schema
# - FrontendError, when the input's use of the schema is valid but the operations it attempts
#   are not supported or not valid
# - InvalidIRQueryError should in principle never be seen in Python. It indicates that
#   the internal representation could not be converted to its "indexed" (i.e. execution-ready) form.
# - InvalidSchemaError, when the provided schema cannot be used due to violating some necessary
#   precondition as explained in the error message.
# - QueryArgumentsError, if the query by itself is fine but cannot be executed together with
#   the provided arguments.
from ._internals import (
    FrontendError,
    InvalidIRQueryError,
    InvalidSchemaError,
    ParseError,
    QueryArgumentsError,
    ValidationError,
)

__all__ = [
    # from .adapter
    "Adapter",
    "Context",
    "FieldValue",
    #
    # from .execution
    "execute_query",
    #
    # from ._internals (defined in Rust)
    "FrontendError",
    "InvalidIRQueryError",
    "InvalidSchemaError",
    "ParseError",
    "QueryArgumentsError",
    "Schema",
    "ValidationError",
]
