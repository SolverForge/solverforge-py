use pyo3::prelude::*;

use super::DynamicSchema;

pub fn schema_debug_name(schema: &DynamicSchema) -> String {
    format!("{}<{}>", schema.solution_type, schema.score_family)
}

pub fn ensure_callable(value: &Bound<'_, PyAny>, label: &str) -> PyResult<()> {
    if value.is_callable() {
        Ok(())
    } else {
        Err(crate::error::py_err(format!("{label} must be callable")))
    }
}
