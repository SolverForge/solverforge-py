use pyo3::create_exception;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

create_exception!(_native, NativeSolverError, PyRuntimeError);

pub fn py_err(message: impl Into<String>) -> PyErr {
    NativeSolverError::new_err(message.into())
}
