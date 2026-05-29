use pyo3::prelude::*;

pub struct CallbackHandle {
    pub callback: Py<PyAny>,
}

unsafe impl Send for CallbackHandle {}
unsafe impl Sync for CallbackHandle {}

impl CallbackHandle {
    pub fn new(callback: Py<PyAny>) -> Self {
        Self { callback }
    }
}
