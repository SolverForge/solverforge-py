use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::error::py_err;

use super::types::{DynamicSchema, EntitySchema, FactSchema, ShadowUpdateSchema, VariableSchema};

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
                variables.push(VariableSchema {
                    name: required_str(field, "name")?,
                    kind,
                    value_range_provider: optional_str(field, "value_range_provider")?,
                    allows_unassigned: optional_bool(field, "allows_unassigned")?.unwrap_or(false),
                    element_collection: optional_str(field, "element_collection")?,
                    element_owner: optional_callable(field, "element_owner")?,
                    route_depot: optional_callable(field, "route_depot")?,
                    route_metric_class: optional_callable(field, "route_metric_class")?,
                    route_distance: optional_callable(field, "route_distance")?,
                    route_feasible: optional_callable(field, "route_feasible")?,
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
        .unbind();
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
        scalar_groups,
        conflict_repairs,
        shadow_updates,
    })
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
