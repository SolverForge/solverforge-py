use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::error::py_err;

use super::types::{
    AssignmentScalarGroupLimitsSchema, AssignmentScalarGroupSchema, DynamicSchema, EntitySchema,
    FactSchema, ShadowUpdateSchema, VariableSchema,
};

#[pyfunction]
pub fn validate_schema(schema: &Bound<'_, PyDict>) -> PyResult<()> {
    parse_schema(schema).map(|_| ())
}

pub fn parse_schema(schema: &Bound<'_, PyDict>) -> PyResult<DynamicSchema> {
    let solution_type = required_str(schema, "solution_type")?;
    let score_family = required_str(schema, "score_family")?;
    let entities_any = schema
        .get_item("entities")?
        .ok_or_else(|| py_err("schema is missing `entities`"))?;
    let entities_list = entities_any.cast::<PyList>()?;
    let mut entities = Vec::new();
    for entity_any in entities_list.iter() {
        let entity = entity_any.cast::<PyDict>()?;
        let type_name = required_str(entity, "type_name")?;
        let collection = required_str(entity, "collection")?;
        let fields_any = entity
            .get_item("fields")?
            .ok_or_else(|| py_err(format!("entity `{type_name}` is missing `fields`")))?;
        let fields = fields_any.cast::<PyList>()?;
        let mut variables = Vec::new();
        for field_any in fields.iter() {
            let field = field_any.cast::<PyDict>()?;
            let kind = required_str(field, "kind")?;
            if kind == "planning_variable" || kind == "planning_list_variable" {
                let name = required_str(field, "name")?;
                let storage_name = format!("__solverforge_{name}");
                variables.push(VariableSchema {
                    name,
                    storage_name,
                    kind,
                    value_range_provider: optional_str(field, "value_range_provider")?,
                    candidate_values: optional_callable(field, "candidate_values")?,
                    nearby_value_candidates: optional_callable(field, "nearby_value_candidates")?,
                    nearby_entity_candidates: optional_callable(field, "nearby_entity_candidates")?,
                    nearby_value_distance_meter: optional_callable(
                        field,
                        "nearby_value_distance_meter",
                    )?,
                    nearby_entity_distance_meter: optional_callable(
                        field,
                        "nearby_entity_distance_meter",
                    )?,
                    allows_unassigned: optional_bool(field, "allows_unassigned")?.unwrap_or(false),
                    element_collection: optional_str(field, "element_collection")?,
                    element_owner: optional_callable(field, "element_owner")?,
                    construction_element_order_key: optional_callable(
                        field,
                        "construction_element_order_key",
                    )?,
                    precedence_duration: optional_callable(field, "precedence_duration")?,
                    precedence_successors: optional_callable(field, "precedence_successors")?,
                    route_depot: optional_callable(field, "route_depot")?,
                    route_depot_entity: optional_callable(field, "route_depot_entity")?,
                    route_depot_field: optional_str(field, "route_depot_field")?,
                    route_metric_class: optional_callable(field, "route_metric_class")?,
                    route_metric_class_entity: optional_callable(
                        field,
                        "route_metric_class_entity",
                    )?,
                    route_metric_class_field: optional_str(field, "route_metric_class_field")?,
                    route_distance: optional_callable(field, "route_distance")?,
                    route_distance_entity: optional_callable(field, "route_distance_entity")?,
                    route_distance_matrix_field: optional_str(
                        field,
                        "route_distance_matrix_field",
                    )?,
                    route_feasible: optional_callable(field, "route_feasible")?,
                    route_feasible_entity: optional_callable(field, "route_feasible_entity")?,
                    route_capacity_field: optional_str(field, "route_capacity_field")?,
                    route_demand_field: optional_str(field, "route_demand_field")?,
                });
            }
        }
        entities.push(EntitySchema {
            type_name,
            collection,
            variables,
        });
    }
    let facts = parse_facts(schema)?;
    let constraints = schema
        .get_item("constraints")?
        .ok_or_else(|| py_err("schema is missing `constraints`"))?
        .unbind();
    let scalar_groups = schema
        .get_item("scalar_groups")?
        .ok_or_else(|| py_err("schema is missing `scalar_groups`"))?
        .clone();
    let assignment_scalar_groups = parse_assignment_scalar_groups(&scalar_groups)?;
    let conflict_repairs = schema
        .get_item("conflict_repairs")?
        .ok_or_else(|| py_err("schema is missing `conflict_repairs`"))?
        .unbind();
    let shadow_updates = parse_shadow_updates(schema)?;
    Ok(DynamicSchema {
        solution_type,
        score_family,
        entities,
        facts,
        constraints,
        scalar_groups: scalar_groups.unbind(),
        assignment_scalar_groups,
        conflict_repairs,
        shadow_updates,
    })
}

fn parse_assignment_scalar_groups(
    scalar_groups_any: &Bound<'_, PyAny>,
) -> PyResult<Vec<AssignmentScalarGroupSchema>> {
    let scalar_groups = scalar_groups_any.cast::<PyList>()?;
    let mut parsed = Vec::new();
    for group_any in scalar_groups.iter() {
        let group = group_any.cast::<PyDict>()?;
        let kind = optional_str(group, "kind")?.unwrap_or_else(|| "callback".to_string());
        if kind != "assignment" {
            continue;
        }
        parsed.push(AssignmentScalarGroupSchema {
            name: required_str(group, "name")?,
            entity_class: required_str(group, "entity_class")?,
            variable_name: required_str(group, "variable_name")?,
            required_entity: optional_callable(group, "required_entity")?,
            capacity_key: optional_callable(group, "capacity_key")?,
            assignment_rule: optional_callable(group, "assignment_rule")?,
            position_key: optional_callable(group, "position_key")?,
            sequence_key: optional_callable(group, "sequence_key")?,
            entity_order: optional_callable(group, "entity_order")?,
            value_order: optional_callable(group, "value_order")?,
            sync_solution_before_callbacks: optional_bool(group, "sync_solution_before_callbacks")?
                .unwrap_or(true),
            limits: AssignmentScalarGroupLimitsSchema {
                value_candidate_limit: optional_limits_usize(group, "value_candidate_limit")?,
                group_candidate_limit: optional_limits_usize(group, "group_candidate_limit")?,
                max_moves_per_step: optional_limits_usize(group, "max_moves_per_step")?,
                max_augmenting_depth: optional_limits_usize(group, "max_augmenting_depth")?,
                max_rematch_size: optional_limits_usize(group, "max_rematch_size")?,
            },
        });
    }
    Ok(parsed)
}

fn parse_facts(schema: &Bound<'_, PyDict>) -> PyResult<Vec<FactSchema>> {
    let Some(facts_any) = schema.get_item("facts")? else {
        return Ok(Vec::new());
    };
    let facts_list = facts_any.cast::<PyList>()?;
    let mut facts = Vec::new();
    for fact_any in facts_list.iter() {
        let fact = fact_any.cast::<PyDict>()?;
        facts.push(FactSchema {
            type_name: required_str(fact, "type_name")?,
            collection: required_str(fact, "collection")?,
        });
    }
    Ok(facts)
}

fn parse_shadow_updates(schema: &Bound<'_, PyDict>) -> PyResult<Vec<ShadowUpdateSchema>> {
    let Some(updates_any) = schema.get_item("shadow_updates")? else {
        return Ok(Vec::new());
    };
    let updates = updates_any.cast::<PyList>()?;
    let mut parsed = Vec::new();
    for update_any in updates.iter() {
        let update = update_any.cast::<PyDict>()?;
        let list_owner = required_str(update, "list_owner")?;
        let Some(listener) = optional_callable(update, "post_update_listener")? else {
            return Err(py_err(format!(
                "shadow update for `{list_owner}` is missing callable `post_update_listener`"
            )));
        };
        parsed.push(ShadowUpdateSchema {
            list_owner,
            post_update_listener: listener,
        });
    }
    Ok(parsed)
}

fn required_str(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<String> {
    dict.get_item(key)?
        .ok_or_else(|| py_err(format!("schema is missing `{key}`")))?
        .extract::<String>()
}

fn optional_str(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<String>> {
    dict.get_item(key)?
        .map(|value| {
            if value.is_none() {
                Ok(None)
            } else {
                value.extract::<String>().map(Some)
            }
        })
        .transpose()
        .map(Option::flatten)
}

fn optional_bool(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<bool>> {
    dict.get_item(key)?
        .map(|value| {
            if value.is_none() {
                Ok(None)
            } else {
                value.extract::<bool>().map(Some)
            }
        })
        .transpose()
        .map(Option::flatten)
}

fn optional_limits_usize(group: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<usize>> {
    let Some(limits_any) = group.get_item("limits")? else {
        return Ok(None);
    };
    if limits_any.is_none() {
        return Ok(None);
    }
    let limits = limits_any.cast::<PyDict>()?;
    limits
        .get_item(key)?
        .map(|value| {
            if value.is_none() {
                Ok(None)
            } else {
                value.extract::<usize>().map(Some)
            }
        })
        .transpose()
        .map(Option::flatten)
}

fn optional_callable(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<Py<PyAny>>> {
    dict.get_item(key)?
        .map(|value| {
            if value.is_none() {
                Ok(None)
            } else if value.is_callable() {
                Ok(Some(value.unbind()))
            } else {
                Err(py_err(format!("`{key}` must be callable when provided")))
            }
        })
        .transpose()
        .map(Option::flatten)
}
