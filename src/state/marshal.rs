use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use solverforge_config::SolverConfig;

use crate::schema::DynamicSchema;
use crate::state::callback_view::PythonCallbackView;
use crate::state::entity_table::{DynamicEntityRow, DynamicState};
use crate::state::PyDynamicSolution;
use crate::value::DynamicValue;

pub fn import_solution(
    py_solution: &Bound<'_, PyAny>,
    schema: Arc<DynamicSchema>,
) -> PyResult<PyDynamicSolution> {
    let solution_fields = import_shared_solution_fields(py_solution, schema.as_ref())?;
    let callback_root_fields = import_callback_root_context(py_solution, schema.as_ref())?;
    let shared_field_names = solution_fields.keys().cloned().collect::<BTreeSet<_>>();
    let route_field_names = route_field_names(schema.as_ref());
    let mut entity_tables = Vec::new();
    let mut entity_objects = Vec::new();
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
        let mut objects = Vec::new();
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
                    if let Some(callback) = variable.candidate_values.as_ref() {
                        let values = callback
                            .bind(py_solution.py())
                            .call1((item.clone(),))?
                            .extract::<Vec<usize>>()?;
                        row.candidates.insert(variable.name.clone(), values);
                    } else if let Some(provider) = variable.value_range_provider.as_ref() {
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
            import_instance_dict_fields(&item, &variable_names, &mut row)?;
            for attr in item.dir()?.iter() {
                let name = attr.extract::<String>()?;
                if name.starts_with('_')
                    || variable_names.contains(name.as_str())
                    || (shared_field_names.contains(name.as_str())
                        && !route_field_names.contains(name.as_str()))
                {
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
            objects.push(item.unbind());
        }
        entity_tables.push(rows);
        entity_objects.push(objects);
        list_elements.push(entity_list_elements);
    }
    let mut fact_tables = Vec::new();
    let mut fact_objects = Vec::new();
    for fact in &schema.facts {
        let collection = py_solution.getattr(fact.collection.as_str())?;
        let collection = collection.cast::<PyList>()?;
        let mut rows = Vec::new();
        let mut objects = Vec::new();
        for item in collection.iter() {
            rows.push(import_fields_from_python(&item, &BTreeSet::new())?);
            objects.push(item.unbind());
        }
        fact_tables.push(rows);
        fact_objects.push(objects);
    }
    let mut solution = PyDynamicSolution {
        schema,
        state: DynamicState {
            entities: entity_tables,
            facts: fact_tables,
            list_elements,
            solution_fields,
        },
        callback_view: PythonCallbackView::from_import(
            py_solution.clone().unbind(),
            entity_objects,
            fact_objects,
            callback_root_fields,
        ),
        score: None,
        solver_config: SolverConfig::default(),
        revision: 0,
    };
    solution.refresh_all_shadows()?;
    Ok(solution)
}

fn import_shared_solution_fields(
    py_solution: &Bound<'_, PyAny>,
    schema: &DynamicSchema,
) -> PyResult<BTreeMap<String, DynamicValue>> {
    let mut field_names = BTreeSet::new();
    for entity in &schema.entities {
        for variable in &entity.variables {
            if let Some(field_name) = variable.route_depot_field.as_ref() {
                field_names.insert(field_name.clone());
            }
            if let Some(field_name) = variable.route_metric_class_field.as_ref() {
                field_names.insert(field_name.clone());
            }
            if let Some(field_name) = variable.route_distance_matrix_field.as_ref() {
                field_names.insert(field_name.clone());
            }
            if let Some(field_name) = variable.route_capacity_field.as_ref() {
                field_names.insert(field_name.clone());
            }
            if let Some(field_name) = variable.route_demand_field.as_ref() {
                field_names.insert(field_name.clone());
            }
        }
    }

    let mut fields = BTreeMap::new();
    for name in field_names {
        let Ok(value) = py_solution.getattr(name.as_str()) else {
            continue;
        };
        if value.is_callable() {
            continue;
        }
        fields.insert(name, DynamicValue::from_python(&value)?);
    }
    Ok(fields)
}

fn import_callback_root_context(
    py_solution: &Bound<'_, PyAny>,
    schema: &DynamicSchema,
) -> PyResult<BTreeMap<String, Py<PyAny>>> {
    let skip_names = callback_root_skip_names(schema);
    let mut fields = BTreeMap::new();
    import_callback_root_dict_fields(py_solution, &skip_names, &mut fields)?;
    for attr in py_solution.dir()?.iter() {
        let name = attr.extract::<String>()?;
        if name.starts_with('_') || skip_names.contains(name.as_str()) {
            continue;
        }
        let Ok(value) = py_solution.getattr(name.as_str()) else {
            continue;
        };
        if value.is_callable() {
            continue;
        }
        fields.insert(name, value.unbind());
    }
    Ok(fields)
}

fn callback_root_skip_names(schema: &DynamicSchema) -> BTreeSet<String> {
    let mut skip_names = BTreeSet::new();
    for entity in &schema.entities {
        skip_names.insert(entity.collection.clone());
    }
    for fact in &schema.facts {
        skip_names.insert(fact.collection.clone());
    }
    skip_names
}

fn import_callback_root_dict_fields(
    py_solution: &Bound<'_, PyAny>,
    skip_names: &BTreeSet<String>,
    fields: &mut BTreeMap<String, Py<PyAny>>,
) -> PyResult<()> {
    let Ok(dict_any) = py_solution.getattr("__dict__") else {
        return Ok(());
    };
    let Ok(dict) = dict_any.cast::<PyDict>() else {
        return Ok(());
    };
    for (key, value) in dict.iter() {
        let name = key.extract::<String>()?;
        if name.starts_with('_') || skip_names.contains(name.as_str()) || value.is_callable() {
            continue;
        }
        fields.insert(name, value.unbind());
    }
    Ok(())
}

fn route_field_names(schema: &DynamicSchema) -> BTreeSet<String> {
    let mut field_names = BTreeSet::new();
    for entity in &schema.entities {
        for variable in &entity.variables {
            if let Some(field_name) = variable.route_depot_field.as_ref() {
                field_names.insert(field_name.clone());
            }
            if let Some(field_name) = variable.route_metric_class_field.as_ref() {
                field_names.insert(field_name.clone());
            }
            if let Some(field_name) = variable.route_distance_matrix_field.as_ref() {
                field_names.insert(field_name.clone());
            }
            if let Some(field_name) = variable.route_capacity_field.as_ref() {
                field_names.insert(field_name.clone());
            }
            if let Some(field_name) = variable.route_demand_field.as_ref() {
                field_names.insert(field_name.clone());
            }
        }
    }
    field_names
}

fn import_instance_dict_fields(
    item: &Bound<'_, PyAny>,
    skip_names: &BTreeSet<&str>,
    row: &mut DynamicEntityRow,
) -> PyResult<()> {
    let Ok(dict_any) = item.getattr("__dict__") else {
        return Ok(());
    };
    let Ok(dict) = dict_any.cast::<PyDict>() else {
        return Ok(());
    };
    for (key, value) in dict.iter() {
        let name = key.extract::<String>()?;
        if name.starts_with('_') || skip_names.contains(name.as_str()) || value.is_callable() {
            continue;
        }
        row.fields.insert(name, DynamicValue::from_python(&value)?);
    }
    Ok(())
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

fn is_read_only_property(item: &Bound<'_, PyAny>, name: &str) -> PyResult<bool> {
    let Ok(class_attr) = item.get_type().getattr(name) else {
        return Ok(false);
    };
    let builtins = item.py().import("builtins")?;
    let property_type = builtins.getattr("property")?;
    if !class_attr.is_instance(&property_type)? {
        return Ok(false);
    }
    Ok(class_attr.getattr("fset")?.is_none())
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
            for name in &row.shadow_fields {
                let Some(value) = row.fields.get(name) else {
                    continue;
                };
                if is_read_only_property(&item, name.as_str())? {
                    continue;
                }
                let value = value.to_python(py_solution.py())?;
                item.setattr(name.as_str(), value.bind(py_solution.py()))?;
            }
        }
    }
    Ok(py_solution.clone().unbind())
}
