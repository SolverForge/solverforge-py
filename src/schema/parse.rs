use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use crate::error::py_err;

use super::types::{
    AssignmentScalarGroupLimitsSchema, AssignmentScalarGroupSchema, DynamicSchema, EntitySchema,
    FactSchema, ListCapacityFeasibilitySchema, ListMetadataFieldSourceSchema, ListMetadataSchema,
    ListMetadataSourceSchema, ListRouteMetadataSchema, ListSavingsMetadataSchema,
    MetadataSourceSchema, ShadowUpdateSchema, VariableSchema,
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
                let list_metadata = if kind == "planning_list_variable" {
                    Some(parse_list_metadata(field)?)
                } else {
                    reject_list_metadata_on_non_list_variable(field)?;
                    None
                };
                let name = required_str(field, "name")?;
                let storage_name = format!("__solverforge_{name}");
                variables.push(VariableSchema {
                    name,
                    storage_name,
                    kind,
                    value_range_provider: optional_str(field, "value_range_provider")?,
                    candidate_values: optional_callable(field, "candidate_values")?,
                    nearby_value_candidates: optional_metadata_source(
                        field,
                        "nearby_value_candidates",
                        "nearby_value_candidates_field",
                    )?,
                    nearby_entity_candidates: optional_metadata_source(
                        field,
                        "nearby_entity_candidates",
                        "nearby_entity_candidates_field",
                    )?,
                    nearby_value_distance_meter: optional_metadata_source(
                        field,
                        "nearby_value_distance_meter",
                        "nearby_value_distance_field",
                    )?,
                    nearby_entity_distance_meter: optional_metadata_source(
                        field,
                        "nearby_entity_distance_meter",
                        "nearby_entity_distance_field",
                    )?,
                    allows_unassigned: optional_bool(field, "allows_unassigned")?.unwrap_or(false),
                    element_collection: optional_str(field, "element_collection")?,
                    element_owner: optional_metadata_source(
                        field,
                        "element_owner",
                        "element_owner_field",
                    )?,
                    construction_element_order: optional_metadata_source(
                        field,
                        "construction_element_order_key",
                        "construction_element_order_field",
                    )?,
                    precedence_duration: optional_metadata_source(
                        field,
                        "precedence_duration",
                        "precedence_duration_field",
                    )?,
                    precedence_successors: optional_metadata_source(
                        field,
                        "precedence_successors",
                        "precedence_successors_field",
                    )?,
                    list_metadata,
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
    let candidate_metrics = schema
        .get_item("candidate_metrics")?
        .ok_or_else(|| py_err("schema is missing `candidate_metrics`"))?
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
        candidate_metrics,
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
            required_entity: optional_metadata_source(
                group,
                "required_entity",
                "required_entity_field",
            )?,
            capacity_key: optional_metadata_source(group, "capacity_key", "capacity_key_field")?,
            assignment_rule: optional_callable(group, "assignment_rule")?,
            same_value_conflict_field: optional_str(group, "same_value_conflict_field")?,
            position_key: optional_metadata_source(group, "position_key", "position_key_field")?,
            sequence_key: optional_metadata_source(group, "sequence_key", "sequence_key_field")?,
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

fn required_non_empty_str(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<String> {
    let value = required_str(dict, key)?;
    if value.is_empty() {
        return Err(py_err(format!("`{key}` must be a non-empty string")));
    }
    Ok(value)
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

fn optional_metadata_source(
    dict: &Bound<'_, PyDict>,
    callback_key: &str,
    field_key: &str,
) -> PyResult<Option<MetadataSourceSchema>> {
    let callback = optional_callable(dict, callback_key)?;
    let field = optional_str(dict, field_key)?;
    match (callback, field) {
        (Some(_), Some(_)) => Err(py_err(format!(
            "`{callback_key}` and `{field_key}` cannot both be configured"
        ))),
        (Some(callback), None) => Ok(Some(MetadataSourceSchema::Callback(Arc::new(callback)))),
        (None, Some(field)) if field.is_empty() => Err(py_err(format!(
            "`{field_key}` must be a non-empty string when provided"
        ))),
        (None, Some(field)) => Ok(Some(MetadataSourceSchema::Row(field))),
        (None, None) => Ok(None),
    }
}

fn required_callable(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Py<PyAny>> {
    let value = dict
        .get_item(key)?
        .ok_or_else(|| py_err(format!("schema is missing `{key}`")))?;
    if value.is_callable() {
        Ok(value.unbind())
    } else {
        Err(py_err(format!("`{key}` must be callable when provided")))
    }
}

/// Compile-time arity validation for raw canonical callback schemas.  Public
/// Python wrappers perform the same validation before serialization; this
/// closes the bypass for callers that construct the raw schema dictionary.
fn validate_list_callback_arity(
    callback: &Py<PyAny>,
    context: &str,
    scope: &str,
    arity: usize,
) -> PyResult<()> {
    Python::attach(|py| {
        let inspect = py.import("inspect").map_err(|_| {
            py_err(format!(
                "`{context}` {scope} callback has an introspection-opaque signature"
            ))
        })?;
        let signature = inspect
            .getattr("signature")?
            .call1((callback.bind(py),))
            .map_err(|_| {
                py_err(format!(
                    "`{context}` {scope} callback has an introspection-opaque signature"
                ))
            })?;
        let arguments = PyTuple::new(py, (0..arity).map(|_| py.None()))?;
        if signature.call_method1("bind", &arguments).is_err() {
            return Err(py_err(format!(
                "`{context}` {scope} callback must accept {arity} positional arguments"
            )));
        }
        Ok(())
    })
}

fn reject_list_metadata_on_non_list_variable(field: &Bound<'_, PyDict>) -> PyResult<()> {
    let Some(value) = field.get_item("list_metadata")? else {
        return Ok(());
    };
    if value.is_none() {
        return Ok(());
    }
    Err(py_err(
        "list_metadata is only valid on planning_list_variable fields",
    ))
}

fn parse_list_metadata(field: &Bound<'_, PyDict>) -> PyResult<ListMetadataSchema> {
    let value = field
        .get_item("list_metadata")?
        .ok_or_else(|| py_err("planning_list_variable schema is missing `list_metadata`"))?;
    if value.is_none() {
        return Err(py_err(
            "planning_list_variable `list_metadata` must be a dictionary",
        ));
    }
    let metadata = value.cast::<PyDict>()?;
    reject_unexpected_keys(
        metadata,
        &[
            "route",
            "savings",
            "cross_position_distance",
            "intra_position_distance",
        ],
        "list_metadata",
    )?;
    Ok(ListMetadataSchema {
        route: parse_optional_route_metadata(metadata)?,
        savings: parse_optional_savings_metadata(metadata)?,
        cross_position_distance: parse_optional_list_value_source(
            metadata,
            "cross_position_distance",
            "list_metadata.cross_position_distance",
            4,
            5,
        )?,
        intra_position_distance: parse_optional_list_value_source(
            metadata,
            "intra_position_distance",
            "list_metadata.intra_position_distance",
            3,
            4,
        )?,
    })
}

fn parse_optional_route_metadata(
    metadata: &Bound<'_, PyDict>,
) -> PyResult<Option<ListRouteMetadataSchema>> {
    let value = required_list_metadata_member(metadata, "route")?;
    if value.is_none() {
        return Ok(None);
    }
    let route = value.cast::<PyDict>()?;
    reject_unexpected_keys(
        route,
        &["depot", "distance", "feasible"],
        "list_metadata.route",
    )?;
    Ok(Some(ListRouteMetadataSchema {
        depot: parse_required_list_value_source(route, "depot", "list_metadata.route.depot", 1, 2)?,
        distance: parse_required_list_value_source(
            route,
            "distance",
            "list_metadata.route.distance",
            3,
            4,
        )?,
        feasible: parse_required_list_feasibility_source(
            route,
            "feasible",
            "list_metadata.route.feasible",
            2,
            3,
        )?,
    }))
}

fn parse_optional_savings_metadata(
    metadata: &Bound<'_, PyDict>,
) -> PyResult<Option<ListSavingsMetadataSchema>> {
    let value = required_list_metadata_member(metadata, "savings")?;
    if value.is_none() {
        return Ok(None);
    }
    let savings = value.cast::<PyDict>()?;
    reject_unexpected_keys(
        savings,
        &["depot", "metric_class", "distance", "feasible"],
        "list_metadata.savings",
    )?;
    Ok(Some(ListSavingsMetadataSchema {
        depot: parse_required_list_value_source(
            savings,
            "depot",
            "list_metadata.savings.depot",
            1,
            2,
        )?,
        metric_class: parse_required_list_value_source(
            savings,
            "metric_class",
            "list_metadata.savings.metric_class",
            1,
            2,
        )?,
        distance: parse_required_list_value_source(
            savings,
            "distance",
            "list_metadata.savings.distance",
            3,
            4,
        )?,
        feasible: parse_required_list_feasibility_source(
            savings,
            "feasible",
            "list_metadata.savings.feasible",
            2,
            3,
        )?,
    }))
}

fn parse_optional_list_value_source(
    metadata: &Bound<'_, PyDict>,
    key: &str,
    context: &str,
    entity_arity: usize,
    solution_arity: usize,
) -> PyResult<Option<ListMetadataSourceSchema>> {
    let value = required_list_metadata_member(metadata, key)?;
    if value.is_none() {
        return Ok(None);
    }
    parse_list_value_source(&value, context, entity_arity, solution_arity).map(Some)
}

fn parse_required_list_value_source(
    metadata: &Bound<'_, PyDict>,
    key: &str,
    context: &str,
    entity_arity: usize,
    solution_arity: usize,
) -> PyResult<ListMetadataSourceSchema> {
    let value = required_list_metadata_member(metadata, key)?;
    if value.is_none() {
        return Err(py_err(format!("`{context}` must be configured")));
    }
    parse_list_value_source(&value, context, entity_arity, solution_arity)
}

fn parse_required_list_feasibility_source(
    metadata: &Bound<'_, PyDict>,
    key: &str,
    context: &str,
    entity_arity: usize,
    solution_arity: usize,
) -> PyResult<ListMetadataSourceSchema> {
    let value = required_list_metadata_member(metadata, key)?;
    if value.is_none() {
        return Err(py_err(format!("`{context}` must be configured")));
    }
    parse_list_feasibility_source(&value, context, entity_arity, solution_arity)
}

fn required_list_metadata_member<'py>(
    metadata: &Bound<'py, PyDict>,
    key: &str,
) -> PyResult<Bound<'py, PyAny>> {
    metadata
        .get_item(key)?
        .ok_or_else(|| py_err(format!("list_metadata is missing `{key}`")))
}

fn parse_list_value_source(
    source_any: &Bound<'_, PyAny>,
    context: &str,
    entity_arity: usize,
    solution_arity: usize,
) -> PyResult<ListMetadataSourceSchema> {
    let source = source_any.cast::<PyDict>()?;
    let kind = required_str(source, "kind")?;
    match kind.as_str() {
        "row" => {
            reject_unexpected_keys(source, &["kind", "field"], context)?;
            Ok(ListMetadataSourceSchema::Row(required_non_empty_str(
                source, "field",
            )?))
        }
        "solution_field" => {
            reject_unexpected_keys(source, &["kind", "field"], context)?;
            Ok(ListMetadataSourceSchema::SolutionField(
                required_non_empty_str(source, "field")?,
            ))
        }
        "entity" => {
            reject_unexpected_keys(source, &["kind", "callback"], context)?;
            let callback = required_callable(source, "callback")?;
            validate_list_callback_arity(&callback, context, "entity", entity_arity)?;
            Ok(ListMetadataSourceSchema::EntityCallback(Arc::new(callback)))
        }
        "solution" => {
            reject_unexpected_keys(source, &["kind", "callback"], context)?;
            let callback = required_callable(source, "callback")?;
            validate_list_callback_arity(&callback, context, "solution", solution_arity)?;
            Ok(ListMetadataSourceSchema::SolutionCallback(Arc::new(
                callback,
            )))
        }
        "capacity" => Err(py_err(format!(
            "`{context}` does not support a capacity source"
        ))),
        _ => Err(py_err(format!(
            "`{context}` has unsupported list metadata source `{kind}`"
        ))),
    }
}

fn parse_list_feasibility_source(
    source_any: &Bound<'_, PyAny>,
    context: &str,
    entity_arity: usize,
    solution_arity: usize,
) -> PyResult<ListMetadataSourceSchema> {
    let source = source_any.cast::<PyDict>()?;
    let kind = required_str(source, "kind")?;
    match kind.as_str() {
        "entity" => {
            reject_unexpected_keys(source, &["kind", "callback"], context)?;
            let callback = required_callable(source, "callback")?;
            validate_list_callback_arity(&callback, context, "entity", entity_arity)?;
            Ok(ListMetadataSourceSchema::EntityCallback(Arc::new(callback)))
        }
        "solution" => {
            reject_unexpected_keys(source, &["kind", "callback"], context)?;
            let callback = required_callable(source, "callback")?;
            validate_list_callback_arity(&callback, context, "solution", solution_arity)?;
            Ok(ListMetadataSourceSchema::SolutionCallback(Arc::new(
                callback,
            )))
        }
        "capacity" => {
            reject_unexpected_keys(source, &["kind", "capacity", "demand"], context)?;
            let capacity = required_list_metadata_member(source, "capacity")?;
            let demand = required_list_metadata_member(source, "demand")?;
            if capacity.is_none() || demand.is_none() {
                return Err(py_err(format!(
                    "`{context}` capacity and demand sources must be configured"
                )));
            }
            Ok(ListMetadataSourceSchema::Capacity(
                ListCapacityFeasibilitySchema {
                    capacity: parse_list_field_source(&capacity, &format!("{context}.capacity"))?,
                    demand: parse_list_field_source(&demand, &format!("{context}.demand"))?,
                },
            ))
        }
        "row" | "solution_field" => Err(py_err(format!(
            "`{context}` does not support a field source; use an explicit capacity source"
        ))),
        _ => Err(py_err(format!(
            "`{context}` has unsupported list feasibility source `{kind}`"
        ))),
    }
}

fn parse_list_field_source(
    source_any: &Bound<'_, PyAny>,
    context: &str,
) -> PyResult<ListMetadataFieldSourceSchema> {
    let source = source_any.cast::<PyDict>()?;
    let kind = required_str(source, "kind")?;
    match kind.as_str() {
        "row" => {
            reject_unexpected_keys(source, &["kind", "field"], context)?;
            Ok(ListMetadataFieldSourceSchema::Row(required_non_empty_str(
                source, "field",
            )?))
        }
        "solution_field" => {
            reject_unexpected_keys(source, &["kind", "field"], context)?;
            Ok(ListMetadataFieldSourceSchema::SolutionField(
                required_non_empty_str(source, "field")?,
            ))
        }
        "entity" | "solution" | "capacity" => Err(py_err(format!(
            "`{context}` must be a row or solution_field source"
        ))),
        _ => Err(py_err(format!(
            "`{context}` has unsupported list metadata field source `{kind}`"
        ))),
    }
}

fn reject_unexpected_keys(
    dict: &Bound<'_, PyDict>,
    allowed: &[&str],
    context: &str,
) -> PyResult<()> {
    for (key, _) in dict.iter() {
        let key = key
            .extract::<String>()
            .map_err(|_| py_err(format!("`{context}` keys must be strings")))?;
        if !allowed.contains(&key.as_str()) {
            return Err(py_err(format!("`{context}` has unsupported key `{key}`")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use pyo3::ffi::c_str;
    use pyo3::types::{PyAnyMethods, PyDict, PyDictMethods, PyList, PyModule};
    use pyo3::Python;

    use super::validate_schema;

    #[test]
    fn raw_canonical_list_callback_schema_rejects_wrong_hook_arity() {
        Python::initialize();
        Python::attach(|py| {
            let callbacks = PyModule::from_code(
                py,
                c_str!("def wrong_depot_arity(route, extra):\n    return 0\n"),
                c_str!("schema_callback_arity.py"),
                c_str!("solverforge_py_schema_callback_arity"),
            )
            .expect("test callback module should load");

            let depot = PyDict::new(py);
            depot.set_item("kind", "entity").unwrap();
            depot
                .set_item("callback", callbacks.getattr("wrong_depot_arity").unwrap())
                .unwrap();

            let distance = PyDict::new(py);
            distance.set_item("kind", "row").unwrap();
            distance.set_item("field", "distance_matrix").unwrap();

            let capacity = PyDict::new(py);
            capacity.set_item("kind", "row").unwrap();
            capacity.set_item("field", "capacity").unwrap();
            let demand = PyDict::new(py);
            demand.set_item("kind", "row").unwrap();
            demand.set_item("field", "demands").unwrap();
            let feasible = PyDict::new(py);
            feasible.set_item("kind", "capacity").unwrap();
            feasible.set_item("capacity", &capacity).unwrap();
            feasible.set_item("demand", &demand).unwrap();

            let route = PyDict::new(py);
            route.set_item("depot", &depot).unwrap();
            route.set_item("distance", &distance).unwrap();
            route.set_item("feasible", &feasible).unwrap();
            let metadata = PyDict::new(py);
            metadata.set_item("route", &route).unwrap();
            metadata.set_item("savings", py.None()).unwrap();
            metadata
                .set_item("cross_position_distance", py.None())
                .unwrap();
            metadata
                .set_item("intra_position_distance", py.None())
                .unwrap();

            let field = PyDict::new(py);
            field.set_item("name", "visits").unwrap();
            field.set_item("kind", "planning_list_variable").unwrap();
            field.set_item("element_collection", "visits").unwrap();
            field.set_item("list_metadata", &metadata).unwrap();
            let entity = PyDict::new(py);
            entity.set_item("type_name", "Route").unwrap();
            entity.set_item("collection", "routes").unwrap();
            entity
                .set_item("fields", PyList::new(py, [&field]).unwrap())
                .unwrap();

            let schema = PyDict::new(py);
            schema.set_item("solution_type", "Plan").unwrap();
            schema.set_item("score_family", "hard_soft").unwrap();
            schema
                .set_item("entities", PyList::new(py, [&entity]).unwrap())
                .unwrap();
            schema.set_item("constraints", py.None()).unwrap();
            schema.set_item("scalar_groups", PyList::empty(py)).unwrap();
            schema
                .set_item("conflict_repairs", PyList::empty(py))
                .unwrap();

            let error = validate_schema(&schema)
                .expect_err("raw canonical callback schema must reject an invalid hook arity");
            assert!(error.to_string().contains(
                "`list_metadata.route.depot` entity callback must accept 1 positional arguments"
            ));
        });
    }
}
