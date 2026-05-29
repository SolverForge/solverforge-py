use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyList;
use solverforge_config::SolverConfig;

use crate::schema::DynamicSchema;
use crate::state::entity_table::{DynamicEntityRow, DynamicState};
use crate::state::PyDynamicSolution;
use crate::value::DynamicValue;

pub fn import_solution(
    py_solution: &Bound<'_, PyAny>,
    schema: Arc<DynamicSchema>,
) -> PyResult<PyDynamicSolution> {
    let mut entity_tables = Vec::new();
    let mut list_elements = Vec::new();
    for entity in &schema.entities {
        let collection = py_solution.getattr(entity.collection.as_str())?;
        let collection = collection.cast::<PyList>()?;
        let variable_names = entity
            .variables
            .iter()
            .map(|variable| variable.name.as_str())
            .collect::<BTreeSet<_>>();
        let mut entity_list_elements = BTreeMap::new();
        let mut rows = Vec::new();
        for item in collection.iter() {
            let mut row = DynamicEntityRow::default();
            for variable in &entity.variables {
                let value = item.getattr(variable.name.as_str())?;
                if variable.kind == "planning_variable" {
                    let scalar = if value.is_none() {
                        None
                    } else {
                        Some(value.extract::<usize>()?)
                    };
                    row.scalars.insert(variable.name.clone(), scalar);
                    if let Some(provider) = variable.value_range_provider.as_ref() {
                        let values = py_solution
                            .getattr(provider.as_str())?
                            .cast::<PyList>()?
                            .iter()
                            .map(|item| item.extract::<usize>())
                            .collect::<PyResult<Vec<_>>>()?;
                        row.candidates.insert(variable.name.clone(), values);
                    }
                } else if variable.kind == "planning_list_variable" {
                    let values = value
                        .cast::<PyList>()?
                        .iter()
                        .map(|item| item.extract::<usize>())
                        .collect::<PyResult<Vec<_>>>()?;
                    row.lists.insert(variable.name.clone(), values);
                    if let Some(element_collection) = variable.element_collection.as_ref() {
                        let values = py_solution
                            .getattr(element_collection.as_str())?
                            .cast::<PyList>()?
                            .iter()
                            .map(|item| item.extract::<usize>())
                            .collect::<PyResult<Vec<_>>>()?;
                        entity_list_elements.insert(variable.name.clone(), values);
                    }
                } else {
                    row.fields
                        .insert(variable.name.clone(), DynamicValue::from_python(&value)?);
                }
            }
            for attr in item.dir()?.iter() {
                let name = attr.extract::<String>()?;
                if name.starts_with('_') || variable_names.contains(name.as_str()) {
                    continue;
                }
                let Ok(value) = item.getattr(name.as_str()) else {
                    continue;
                };
                if value.is_callable() {
                    continue;
                }
                row.fields.insert(name, DynamicValue::from_python(&value)?);
            }
            rows.push(row);
        }
        entity_tables.push(rows);
        list_elements.push(entity_list_elements);
    }
    let mut fact_tables = Vec::new();
    for fact in &schema.facts {
        let collection = py_solution.getattr(fact.collection.as_str())?;
        let collection = collection.cast::<PyList>()?;
        let mut rows = Vec::new();
        for item in collection.iter() {
            rows.push(import_fields_from_python(&item, &BTreeSet::new())?);
        }
        fact_tables.push(rows);
    }
    Ok(PyDynamicSolution {
        schema,
        state: DynamicState {
            entities: entity_tables,
            facts: fact_tables,
            list_elements,
        },
        score: None,
        solver_config: SolverConfig::default(),
    })
}

fn import_fields_from_python(
    item: &Bound<'_, PyAny>,
    skip_names: &BTreeSet<&str>,
) -> PyResult<DynamicEntityRow> {
    let mut row = DynamicEntityRow::default();
    for attr in item.dir()?.iter() {
        let name = attr.extract::<String>()?;
        if name.starts_with('_') || skip_names.contains(name.as_str()) {
            continue;
        }
        let Ok(value) = item.getattr(name.as_str()) else {
            continue;
        };
        if value.is_callable() {
            continue;
        }
        row.fields.insert(name, DynamicValue::from_python(&value)?);
    }
    Ok(row)
}

pub fn export_solution(
    py_solution: &Bound<'_, PyAny>,
    solution: &PyDynamicSolution,
) -> PyResult<Py<PyAny>> {
    for (entity_index, entity) in solution.schema.entities.iter().enumerate() {
        let collection = py_solution.getattr(entity.collection.as_str())?;
        let collection = collection.cast::<PyList>()?;
        for (row_index, item) in collection.iter().enumerate() {
            let row = &solution.state.entities[entity_index][row_index];
            for variable in &entity.variables {
                if variable.kind == "planning_variable" {
                    match row.scalars.get(variable.name.as_str()).copied().flatten() {
                        Some(value) => item.setattr(variable.name.as_str(), value)?,
                        None => item.setattr(variable.name.as_str(), py_solution.py().None())?,
                    }
                } else if variable.kind == "planning_list_variable" {
                    if let Some(values) = row.lists.get(variable.name.as_str()) {
                        item.setattr(variable.name.as_str(), values.clone())?;
                    }
                }
            }
        }
    }
    Ok(py_solution.clone().unbind())
}
