pub mod callbacks;
pub mod config;
pub mod constraints;
pub mod descriptor;
pub mod error;
pub mod intern;
pub mod manager;
pub mod proxy;
pub mod runtime;
pub mod schema;
pub mod score;
pub mod solver;
pub mod state;
pub mod value;

use pyo3::prelude::*;

#[pyfunction]
fn native_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pyfunction]
fn init_console() {
    solverforge_console::init();
}

#[pymodule(gil_used = false)]
fn _native(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add(
        "NativeSolverError",
        py.get_type::<error::NativeSolverError>(),
    )?;
    module.add_function(wrap_pyfunction!(native_version, module)?)?;
    module.add_function(wrap_pyfunction!(init_console, module)?)?;
    module.add_function(wrap_pyfunction!(solver::solve, module)?)?;
    module.add_function(wrap_pyfunction!(solver::calculate_score, module)?)?;
    module.add_function(wrap_pyfunction!(schema::validate_schema, module)?)?;
    module.add_class::<manager::NativeSolverManager>()?;
    Ok(())
}
