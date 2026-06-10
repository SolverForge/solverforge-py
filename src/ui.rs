use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

#[pyfunction]
pub fn ui_asset(py: Python<'_>, path: &str) -> PyResult<Option<Py<PyAny>>> {
    let Some(asset) = solverforge_ui::assets::get(path) else {
        return Ok(None);
    };

    let dict = PyDict::new(py);
    dict.set_item("path", path)?;
    dict.set_item("content_type", asset.content_type)?;
    dict.set_item("cache_control", asset.cache_control)?;
    dict.set_item("bytes", PyBytes::new(py, asset.bytes))?;
    Ok(Some(dict.into_any().unbind()))
}

#[pyfunction]
pub fn ui_asset_paths() -> Vec<&'static str> {
    solverforge_ui::assets::paths()
}
