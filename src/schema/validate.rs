use crate::error::py_err;

use super::DynamicSchema;

pub fn validate_dynamic_schema(schema: &DynamicSchema) -> Result<(), pyo3::PyErr> {
    if schema.entities.is_empty() {
        return Err(py_err(
            "planning solution must contain at least one entity collection",
        ));
    }
    for entity in &schema.entities {
        if entity.variables.is_empty() {
            return Err(py_err(format!(
                "entity `{}` has no planning variables",
                entity.type_name
            )));
        }
    }
    Ok(())
}
