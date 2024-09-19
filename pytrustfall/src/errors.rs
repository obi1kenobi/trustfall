use pyo3::{
    create_exception,
    types::{PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

create_exception!(_trustfall_internal, InvalidSchemaError, pyo3::exceptions::PyException);
create_exception!(_trustfall_internal, ParseError, pyo3::exceptions::PyException);
create_exception!(_trustfall_internal, ValidationError, pyo3::exceptions::PyException);
create_exception!(_trustfall_internal, FrontendError, pyo3::exceptions::PyException);
create_exception!(_trustfall_internal, InvalidIRQueryError, pyo3::exceptions::PyException);
create_exception!(_trustfall_internal, QueryArgumentsError, pyo3::exceptions::PyException);

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("InvalidSchemaError", py.get_type_bound::<InvalidSchemaError>())?;
    m.add("ParseError", py.get_type_bound::<ParseError>())?;
    m.add("ValidationError", py.get_type_bound::<ValidationError>())?;
    m.add("FrontendError", py.get_type_bound::<FrontendError>())?;
    m.add("InvalidIRQueryError", py.get_type_bound::<InvalidIRQueryError>())?;
    m.add("QueryArgumentsError", py.get_type_bound::<QueryArgumentsError>())?;
    Ok(())
}
