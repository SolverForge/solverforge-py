use crate::error::py_err;

use super::types::{ListMetadataSchema, ListMetadataSourceSchema};
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
        for variable in &entity.variables {
            if variable.kind == "planning_list_variable" {
                let metadata = variable.list_metadata.as_ref().ok_or_else(|| {
                    py_err(format!(
                        "planning_list_variable `{}.{}` is missing canonical list_metadata",
                        entity.type_name, variable.name
                    ))
                })?;
                validate_list_metadata(metadata, &entity.type_name, &variable.name)?;
                continue;
            }
            if variable.element_collection.is_some()
                || variable.element_owner.is_some()
                || variable.construction_element_order.is_some()
                || variable.precedence_duration.is_some()
                || variable.precedence_successors.is_some()
                || variable.list_metadata.is_some()
            {
                return Err(py_err(format!(
                    "list metadata is only valid on planning_list_variable `{}.{}`",
                    entity.type_name, variable.name
                )));
            }
        }
    }
    let mut assignment_group_names = std::collections::BTreeSet::new();
    for group in &schema.assignment_scalar_groups {
        if group.name.is_empty() {
            return Err(py_err("assignment scalar group has an empty name"));
        }
        if !assignment_group_names.insert(group.name.as_str()) {
            return Err(py_err(format!(
                "assignment scalar group `{}` is declared more than once",
                group.name
            )));
        }
        if group.assignment_rule.is_some() && group.sequence_key.is_none() {
            return Err(py_err(format!(
                "assignment scalar group `{}` declares assignment_rule but no sequence_key",
                group.name
            )));
        }
        if group.assignment_rule.is_some() && group.same_value_conflict_field.is_some() {
            return Err(py_err(format!(
                "assignment scalar group `{}` cannot declare both assignment_rule and same_value_conflict_field",
                group.name
            )));
        }
        if group
            .same_value_conflict_field
            .as_ref()
            .is_some_and(String::is_empty)
        {
            return Err(py_err(format!(
                "assignment scalar group `{}` has an empty same_value_conflict_field",
                group.name
            )));
        }
        if group.same_value_conflict_field.is_some() && group.sequence_key.is_none() {
            return Err(py_err(format!(
                "assignment scalar group `{}` declares same_value_conflict_field but no sequence_key",
                group.name
            )));
        }
    }
    Ok(())
}

fn validate_list_metadata(
    metadata: &ListMetadataSchema,
    entity_type: &str,
    variable_name: &str,
) -> Result<(), pyo3::PyErr> {
    let target = format!("{entity_type}.{variable_name}");
    if let Some(route) = metadata.route.as_ref() {
        validate_list_value_source(&route.depot, &target, "route.depot")?;
        validate_list_value_source(&route.distance, &target, "route.distance")?;
        validate_list_feasibility_source(&route.feasible, &target, "route.feasible")?;
    }
    if let Some(savings) = metadata.savings.as_ref() {
        validate_list_value_source(&savings.depot, &target, "savings.depot")?;
        validate_list_value_source(&savings.metric_class, &target, "savings.metric_class")?;
        validate_list_value_source(&savings.distance, &target, "savings.distance")?;
        validate_list_feasibility_source(&savings.feasible, &target, "savings.feasible")?;
    }
    if let Some(source) = metadata.cross_position_distance.as_ref() {
        validate_list_value_source(source, &target, "cross_position_distance")?;
    }
    if let Some(source) = metadata.intra_position_distance.as_ref() {
        validate_list_value_source(source, &target, "intra_position_distance")?;
    }
    Ok(())
}

fn validate_list_value_source(
    source: &ListMetadataSourceSchema,
    target: &str,
    source_name: &str,
) -> Result<(), pyo3::PyErr> {
    if matches!(source, ListMetadataSourceSchema::Capacity(_)) {
        return Err(py_err(format!(
            "planning list `{target}` {source_name} must not use a capacity source"
        )));
    }
    Ok(())
}

fn validate_list_feasibility_source(
    source: &ListMetadataSourceSchema,
    target: &str,
    source_name: &str,
) -> Result<(), pyo3::PyErr> {
    if matches!(
        source,
        ListMetadataSourceSchema::Row(_) | ListMetadataSourceSchema::SolutionField(_)
    ) {
        return Err(py_err(format!(
            "planning list `{target}` {source_name} must use an entity callback, solution callback, or capacity source"
        )));
    }
    Ok(())
}
