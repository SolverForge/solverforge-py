use pyo3::prelude::*;

#[pyclass]
#[derive(Clone)]
pub struct EntityProxy {
    #[pyo3(get)]
    pub descriptor_index: usize,
    #[pyo3(get)]
    pub entity_index: usize,
}
