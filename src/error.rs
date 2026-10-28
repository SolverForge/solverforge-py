use std::any::Any;
use std::panic::{panic_any, set_hook, take_hook};
use std::sync::Once;

use pyo3::create_exception;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use solverforge_solver::SolverPanicPayload;

create_exception!(_native, NativeSolverError, PyRuntimeError);

pub fn py_err(message: impl Into<String>) -> PyErr {
    NativeSolverError::new_err(message.into())
}

pub fn panic_with_py_err<T>(error: PyErr) -> T {
    let message = Python::attach(|py| {
        py.import("traceback")
            .and_then(|module| module.call_method1("format_exception", (error.value(py),)))
            .and_then(|formatted| formatted.extract::<Vec<String>>())
            .map(|lines| lines.concat())
            .unwrap_or_else(|_| error.to_string())
    });
    panic_any(SolverPanicPayload::new(message, error))
}

/// Keeps std's default hook from printing `thread panicked at src/error.rs`
/// for a callback error unwinding through the solver. That panic only carries
/// the Python error to `panic_to_py_err` (or the retained job's failure
/// status), which reports it once; every other panic reaches the previous hook.
pub fn install_callback_panic_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = take_hook();
        set_hook(Box::new(move |info| {
            if !info.payload().is::<SolverPanicPayload>() {
                previous(info);
            }
        }));
    });
}

pub fn panic_to_py_err(payload: Box<dyn Any + Send>) -> PyErr {
    let payload = match payload.downcast::<SolverPanicPayload>() {
        Ok(payload) => {
            let (message, foreign) = payload.into_parts();
            return foreign
                .downcast::<PyErr>()
                .map(|error| *error)
                .unwrap_or_else(|_| py_err(message));
        }
        Err(payload) => payload,
    };
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        py_err(*message)
    } else if let Some(message) = payload.downcast_ref::<String>() {
        py_err(message.clone())
    } else {
        py_err("solver panicked with a non-string payload")
    }
}
