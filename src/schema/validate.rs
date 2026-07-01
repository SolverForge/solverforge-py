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
    for group in &schema.assignment_scalar_groups {
        if group.name.is_empty() {
            return Err(py_err("assignment scalar group has an empty name"));
        }
        if group.assignment_rule.is_some() && group.sequence_key.is_none() {
            return Err(py_err(format!(
                "assignment scalar group `{}` declares assignment_rule but no sequence_key",
                group.name
            )));
        }
    }
    Ok(())
}
