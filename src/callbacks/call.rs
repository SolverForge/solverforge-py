use pyo3::prelude::*;

pub fn call_bool(callback: &Bound<'_, PyAny>, arg: &Bound<'_, PyAny>) -> PyResult<bool> {
    callback.call1((arg,))?.extract::<bool>()
}
