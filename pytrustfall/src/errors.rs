use pyo3::{create_exception, types::PyModule, PyResult, Python};

create_exception!(
    pytrustfall,
    InvalidSchemaError,
    pyo3::exceptions::PyException
);
create_exception!(pytrustfall, ParseError, pyo3::exceptions::PyException);
create_exception!(pytrustfall, ValidationError, pyo3::exceptions::PyException);
create_exception!(pytrustfall, FrontendError, pyo3::exceptions::PyException);
create_exception!(
    pytrustfall,
    InvalidIRQueryError,
    pyo3::exceptions::PyException
);
create_exception!(
    pytrustfall,
    QueryArgumentsError,
    pyo3::exceptions::PyException
);

pub(crate) fn register(py: Python, m: &PyModule) -> PyResult<()> {
    m.add("InvalidSchemaError", py.get_type::<InvalidSchemaError>())?;
    m.add("ParseError", py.get_type::<ParseError>())?;
    m.add("ValidationError", py.get_type::<ValidationError>())?;
    m.add("FrontendError", py.get_type::<FrontendError>())?;
    m.add("InvalidIRQueryError", py.get_type::<InvalidIRQueryError>())?;
    m.add("QueryArgumentsError", py.get_type::<QueryArgumentsError>())?;
    Ok(())
}
