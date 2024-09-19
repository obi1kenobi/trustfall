from . import trustfall

# Workaround around issue preventing direct `from ._trustfall_internal import X` imports:
# https://github.com/PyO3/pyo3/issues/759
# https://github.com/PyO3/pyo3/issues/1517
_trustfall_internal = trustfall._trustfall_internal
AdapterShim = _trustfall_internal.AdapterShim
Schema = _trustfall_internal.Schema
FrontendError = _trustfall_internal.FrontendError
InvalidIRQueryError = _trustfall_internal.InvalidIRQueryError
InvalidSchemaError = _trustfall_internal.InvalidSchemaError
ParseError = _trustfall_internal.ParseError
QueryArgumentsError = _trustfall_internal.QueryArgumentsError
ValidationError = _trustfall_internal.ValidationError
interpret_query = _trustfall_internal.interpret_query

__all__ = [
    "AdapterShim",
    "FrontendError",
    "InvalidIRQueryError",
    "InvalidSchemaError",
    "ParseError",
    "QueryArgumentsError",
    "Schema",
    "ValidationError",
    "interpret_query",
]
