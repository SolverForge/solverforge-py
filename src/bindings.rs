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
        py.get_type::<crate::error::NativeSolverError>(),
    )?;
    module.add_function(wrap_pyfunction!(native_version, module)?)?;
    module.add_function(wrap_pyfunction!(init_console, module)?)?;
    module.add_function(wrap_pyfunction!(crate::solver::api::solve, module)?)?;
    module.add_function(wrap_pyfunction!(
        crate::solver::api::calculate_score,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        crate::schema::parse::validate_schema,
        module
    )?)?;
    module.add_class::<crate::manager::NativeSolverManager>()?;
    Ok(())
}
