use pyo3::{Python, types::PyModule, PyResult, pymodule};

pub mod shim;
pub mod errors;

#[pymodule]
fn pytrustfall(py: Python, m: &PyModule) -> PyResult<()> {
    shim::register(py, m)?;
    errors::register(py, m)?;
    Ok(())
}
