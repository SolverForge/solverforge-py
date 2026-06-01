use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::error::py_err;

use super::types::{DynamicSchema, EntitySchema, FactSchema, VariableSchema};

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
    Ok(DynamicSchema {
        solution_type,
        score_family,
        entities,
        facts,
        constraints,
        scalar_groups,
        conflict_repairs,
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
