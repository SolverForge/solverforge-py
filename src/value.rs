use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};

#[derive(Debug, Clone, PartialEq)]
pub enum DynamicValue {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<DynamicValue>),
}

impl DynamicValue {
    pub fn from_python(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if value.is_none() {
            return Ok(Self::None);
        }
        if let Ok(v) = value.extract::<bool>() {
            return Ok(Self::Bool(v));
        }
        if let Ok(v) = value.extract::<i64>() {
            return Ok(Self::Int(v));
        }
        if let Ok(v) = value.extract::<f64>() {
            return Ok(Self::Float(v));
        }
        if let Ok(list) = value.cast::<PyList>() {
            let values = list
                .iter()
                .map(|item| Self::from_python(&item))
                .collect::<PyResult<Vec<_>>>()?;
            return Ok(Self::List(values));
        }
        if let Ok(tuple) = value.cast::<PyTuple>() {
            let values = tuple
                .iter()
                .map(|item| Self::from_python(&item))
                .collect::<PyResult<Vec<_>>>()?;
            return Ok(Self::List(values));
        }
        if let Ok(v) = value.extract::<String>() {
            return Ok(Self::String(v));
        }
        Ok(Self::String(format!("{value:?}")))
    }

    pub fn to_python(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self {
            Self::None => Ok(py.None()),
            Self::Bool(value) => Ok(value.into_pyobject(py)?.to_owned().into_any().unbind()),
            Self::Int(value) => Ok(value.into_pyobject(py)?.into_any().unbind()),
            Self::Float(value) => Ok(value.into_pyobject(py)?.into_any().unbind()),
            Self::String(value) => Ok(value.into_pyobject(py)?.into_any().unbind()),
            Self::List(values) => {
                let items = values
                    .iter()
                    .map(|value| value.to_python(py))
                    .collect::<PyResult<Vec<_>>>()?;
                Ok(PyList::new(py, items)?.into_any().unbind())
            }
        }
    }
}
