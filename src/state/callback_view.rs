use pyo3::prelude::*;

use crate::state::PyDynamicSolution;

#[derive(Debug, Default)]
pub struct PythonCallbackView {
    python_solution: Option<Py<PyAny>>,
    entity_objects: Vec<Vec<Py<PyAny>>>,
    fact_objects: Vec<Vec<Py<PyAny>>>,
}

impl Clone for PythonCallbackView {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PythonCallbackView {
    pub fn from_import(
        python_solution: Py<PyAny>,
        entity_objects: Vec<Vec<Py<PyAny>>>,
        fact_objects: Vec<Vec<Py<PyAny>>>,
    ) -> Self {
        Self {
            python_solution: Some(python_solution),
            entity_objects,
            fact_objects,
        }
    }

    pub fn solution_view(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<Py<PyAny>> {
        if let Some(py_solution) = self.python_solution.as_ref() {
            self.sync_all_python_objects(py, solution)?;
            return Ok(py_solution.clone_ref(py));
        }
        solution.to_python_snapshot(py)
    }

    pub fn unsynced_solution_view(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<Py<PyAny>> {
        if let Some(py_solution) = self.python_solution.as_ref() {
            return Ok(py_solution.clone_ref(py));
        }
        solution.to_python_snapshot(py)
    }

    pub fn entity_view(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
        entity_index: usize,
        row_index: usize,
    ) -> PyResult<Py<PyAny>> {
        if let Some(row) = self.sync_entity_object(py, solution, entity_index, row_index)? {
            return Ok(row);
        }
        let row = solution
            .state
            .entities
            .get(entity_index)
            .and_then(|rows| rows.get(row_index))
            .ok_or_else(|| {
                crate::error::py_err(format!(
                    "dynamic callback row `{row_index}` is out of bounds for entity descriptor `{entity_index}`"
                ))
            })?;
        solution.entity_row_snapshot(py, entity_index, row_index, row)
    }

    fn sync_all_python_objects(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<()> {
        for entity_index in 0..solution.state.entities.len() {
            for row_index in 0..solution.state.entities[entity_index].len() {
                let _ = self.sync_entity_object(py, solution, entity_index, row_index)?;
            }
        }
        for fact_index in 0..solution.state.facts.len() {
            for row_index in 0..solution.state.facts[fact_index].len() {
                self.sync_fact_object(py, solution, fact_index, row_index)?;
            }
        }
        Ok(())
    }

    fn sync_entity_object(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
        entity_index: usize,
        row_index: usize,
    ) -> PyResult<Option<Py<PyAny>>> {
        let Some(object) = self
            .entity_objects
            .get(entity_index)
            .and_then(|rows| rows.get(row_index))
        else {
            return Ok(None);
        };
        let Some(entity_schema) = solution.schema.entities.get(entity_index) else {
            return Ok(None);
        };
        let Some(row) = solution
            .state
            .entities
            .get(entity_index)
            .and_then(|rows| rows.get(row_index))
        else {
            return Ok(None);
        };
        let object_bound = object.bind(py);
        object_bound.setattr("_solverforge_entity_index", row_index)?;
        object_bound.setattr("_solverforge_descriptor_index", entity_index)?;
        object_bound.setattr(
            "_solverforge_entity_class",
            entity_schema.type_name.as_str(),
        )?;
        for variable in &entity_schema.variables {
            match variable.kind.as_str() {
                "planning_variable" => match row.scalars.get(variable.name.as_str()) {
                    Some(Some(value)) => {
                        object_bound.setattr(variable.storage_name.as_str(), *value)?
                    }
                    _ => object_bound.setattr(variable.storage_name.as_str(), py.None())?,
                },
                "planning_list_variable" => {
                    let values = row
                        .lists
                        .get(variable.name.as_str())
                        .cloned()
                        .unwrap_or_default();
                    object_bound.setattr(variable.storage_name.as_str(), values)?;
                }
                _ => {}
            }
        }
        for name in &row.shadow_fields {
            let Some(value) = row.fields.get(name) else {
                continue;
            };
            let value = value.to_python(py)?;
            object_bound.setattr(name.as_str(), value.bind(py))?;
        }
        Ok(Some(object.clone_ref(py)))
    }

    fn sync_fact_object(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
        fact_index: usize,
        row_index: usize,
    ) -> PyResult<()> {
        let Some(object) = self
            .fact_objects
            .get(fact_index)
            .and_then(|rows| rows.get(row_index))
        else {
            return Ok(());
        };
        let Some(fact_schema) = solution.schema.facts.get(fact_index) else {
            return Ok(());
        };
        let object_bound = object.bind(py);
        object_bound.setattr("_solverforge_fact_index", row_index)?;
        object_bound.setattr("_solverforge_fact_class", fact_schema.type_name.as_str())?;
        Ok(())
    }
}
