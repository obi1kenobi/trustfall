#![forbid(unused_lifetimes)]
#![forbid(elided_lifetimes_in_paths)]
#![forbid(clippy::undocumented_unsafe_blocks)]

use pyo3::{
    pymodule,
    types::{PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

pub mod errors;
pub mod shim;
mod value;

fn _trustfall_internal(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    shim::register(py, m)?;
    errors::register(py, m)?;
    Ok(())
}

#[pymodule]
fn trustfall(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let submodule: Bound<'_, PyModule> = PyModule::new(py, "_trustfall_internal")?;
    _trustfall_internal(py, &submodule)?;
    m.add_submodule(&submodule)
}
