#![forbid(unused_lifetimes)]
#![forbid(elided_lifetimes_in_paths)]

use pyo3::{pymodule, types::PyModule, PyResult, Python};

pub mod errors;
pub mod shim;

#[pymodule]
fn trustfall(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    shim::register(py, m)?;
    errors::register(py, m)?;
    Ok(())
}
