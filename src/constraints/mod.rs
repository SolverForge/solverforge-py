pub mod evaluate;
pub mod incremental;
pub mod matches;
pub mod state;
pub mod stream_plan;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use solverforge_core::ConstraintRef;

use crate::error::py_err;

pub use state::PyDynamicConstraintSet;

pub fn constraint_ref(name: &str) -> ConstraintRef {
    ConstraintRef::new("python", name)
}

pub fn constraint_name(plan: &Bound<'_, PyDict>) -> PyResult<String> {
    plan.get_item("name")?
        .ok_or_else(|| py_err("constraint plan missing `name`"))?
        .extract::<String>()
}
