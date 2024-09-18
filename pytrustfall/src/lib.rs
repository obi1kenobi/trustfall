#![forbid(unused_lifetimes)]
#![forbid(elided_lifetimes_in_paths)]

use pyo3::{pymodule, types::PyModule, Bound, PyResult, Python};

pub mod errors;
pub mod shim;
mod value;

#[pymodule]
fn trustfall(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    shim::register(py, m)?;
    errors::register(py, m)?;
    Ok(())
}
