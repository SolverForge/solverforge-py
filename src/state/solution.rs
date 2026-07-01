use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use solverforge_bridge::{DynamicModelBackend, EntityClassId, VariableId};
use solverforge_config::SolverConfig;
use solverforge_core::domain::PlanningSolution;

use crate::schema::DynamicSchema;
use crate::score::DynamicScore;
use crate::state::callback_view::PythonCallbackView;
use crate::state::entity_table::DynamicState;
use crate::value::DynamicValue;

#[derive(Debug)]
pub struct PyDynamicSolution {
    pub schema: Arc<DynamicSchema>,
    pub state: DynamicState,
    pub callback_view: PythonCallbackView,
    pub score: Option<DynamicScore>,
    pub solver_config: SolverConfig,
}

impl PyDynamicSolution {
    pub fn entity_count(&self, descriptor_index: usize) -> usize {
        self.state
            .entities
            .get(descriptor_index)
            .map(Vec::len)
            .unwrap_or(0)
    }

    fn variable_name(&self, entity: EntityClassId, variable: VariableId) -> Option<&str> {
        self.schema
            .entities
            .get(entity.0)?
            .variables
            .get(variable.0)
            .map(|variable| variable.name.as_str())
    }

    pub fn refresh_all_shadows(&mut self) -> PyResult<()> {
        let targets = self
            .schema
            .entities
            .iter()
            .enumerate()
            .filter_map(|(descriptor_index, entity)| {
                self.schema
                    .shadow_updates
                    .iter()
                    .any(|update| update.list_owner == entity.collection)
                    .then_some(descriptor_index)
            })
            .collect::<Vec<_>>();
        for descriptor_index in targets {
            self.refresh_entity_collection_shadows(descriptor_index)?;
        }
        Ok(())
    }

    fn entity_has_shadow_updates(&self, descriptor_index: usize) -> bool {
        let Some(entity_schema) = self.schema.entities.get(descriptor_index) else {
            return false;
        };
        self.schema
            .shadow_updates
            .iter()
            .any(|update| update.list_owner == entity_schema.collection)
    }

    pub fn refresh_entity_collection_shadows(&mut self, descriptor_index: usize) -> PyResult<()> {
        if !self.entity_has_shadow_updates(descriptor_index) {
            return Ok(());
        }
        let entity_count = self.entity_count(descriptor_index);
        for entity_index in 0..entity_count {
            self.refresh_entity_shadows(descriptor_index, entity_index)?;
        }
        Ok(())
    }

    pub fn refresh_entity_shadows(
        &mut self,
        descriptor_index: usize,
        entity_index: usize,
    ) -> PyResult<()> {
        let Some(entity_schema) = self.schema.entities.get(descriptor_index) else {
            return Ok(());
        };
        let collection = entity_schema.collection.clone();
        Python::attach(|py| -> PyResult<()> {
            let callbacks = self
                .schema
                .shadow_updates
                .iter()
                .filter(|update| update.list_owner == collection)
                .map(|update| update.post_update_listener.clone_ref(py))
                .collect::<Vec<_>>();
            for callback in callbacks {
                let callback_view = self.to_python_callback_view(py)?;
                let result = callback.bind(py).call1((callback_view, entity_index))?;
                if result.is_none() {
                    continue;
                }
                let updates = result.cast::<PyDict>()?;
                let Some(row) = self
                    .state
                    .entities
                    .get_mut(descriptor_index)
                    .and_then(|rows| rows.get_mut(entity_index))
                else {
                    continue;
                };
                for item in updates.iter() {
                    let (key, value) = item;
                    let key = key.extract::<String>()?;
                    row.fields
                        .insert(key.clone(), DynamicValue::from_python(&value)?);
                    row.shadow_fields.insert(key);
                }
            }
            Ok(())
        })
    }

    pub fn to_python_snapshot(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let kwargs = PyDict::new(py);
        for (name, value) in &self.state.solution_fields {
            let value = value.to_python(py)?;
            kwargs.set_item(name.as_str(), value.bind(py))?;
        }
        for (entity_index, entity_schema) in self.schema.entities.iter().enumerate() {
            let mut rows = Vec::new();
            if let Some(entity_rows) = self.state.entities.get(entity_index) {
                for (row_index, row) in entity_rows.iter().enumerate() {
                    rows.push(self.entity_row_snapshot(py, entity_index, row_index, row)?);
                }
            }
            kwargs.set_item(entity_schema.collection.as_str(), PyList::new(py, rows)?)?;
        }
        for (fact_index, fact_schema) in self.schema.facts.iter().enumerate() {
            let mut rows = Vec::new();
            if let Some(fact_rows) = self.state.facts.get(fact_index) {
                for (row_index, row) in fact_rows.iter().enumerate() {
                    rows.push(self.fact_row_snapshot(
                        py,
                        fact_schema.type_name.as_str(),
                        row_index,
                        row,
                    )?);
                }
            }
            kwargs.set_item(fact_schema.collection.as_str(), PyList::new(py, rows)?)?;
        }
        let types = py.import("types")?;
        let namespace = types.getattr("SimpleNamespace")?;
        namespace.call((), Some(&kwargs)).map(Bound::unbind)
    }

    pub fn to_python_callback_view(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.callback_view.solution_view(py, self)
    }

    pub fn to_python_unsynced_callback_view(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.callback_view.unsynced_solution_view(py, self)
    }

    pub fn entity_callback_view(
        &self,
        py: Python<'_>,
        entity_index: usize,
        row_index: usize,
    ) -> PyResult<Py<PyAny>> {
        self.callback_view
            .entity_view(py, self, entity_index, row_index)
    }

    pub(crate) fn entity_row_snapshot(
        &self,
        py: Python<'_>,
        entity_index: usize,
        row_index: usize,
        row: &crate::state::entity_table::DynamicEntityRow,
    ) -> PyResult<Py<PyAny>> {
        let kwargs = PyDict::new(py);
        kwargs.set_item("_solverforge_entity_index", row_index)?;
        kwargs.set_item("_solverforge_descriptor_index", entity_index)?;
        kwargs.set_item(
            "_solverforge_entity_class",
            self.schema.entities[entity_index].type_name.as_str(),
        )?;
        for (name, value) in &row.fields {
            let value = value.to_python(py)?;
            kwargs.set_item(name, value.bind(py))?;
        }
        for variable in &self.schema.entities[entity_index].variables {
            match variable.kind.as_str() {
                "planning_variable" => match row.scalars.get(variable.name.as_str()) {
                    Some(Some(value)) => kwargs.set_item(variable.name.as_str(), *value)?,
                    _ => kwargs.set_item(variable.name.as_str(), py.None())?,
                },
                "planning_list_variable" => {
                    let values = row
                        .lists
                        .get(variable.name.as_str())
                        .cloned()
                        .unwrap_or_default();
                    kwargs.set_item(variable.name.as_str(), values)?;
                }
                _ => {}
            }
        }
        let types = py.import("types")?;
        let namespace = types.getattr("SimpleNamespace")?;
        namespace.call((), Some(&kwargs)).map(Bound::unbind)
    }

    pub(crate) fn fact_row_snapshot(
        &self,
        py: Python<'_>,
        fact_type_name: &str,
        row_index: usize,
        row: &crate::state::entity_table::DynamicEntityRow,
    ) -> PyResult<Py<PyAny>> {
        let kwargs = PyDict::new(py);
        kwargs.set_item("_solverforge_fact_index", row_index)?;
        kwargs.set_item("_solverforge_fact_class", fact_type_name)?;
        for (name, value) in &row.fields {
            let value = value.to_python(py)?;
            kwargs.set_item(name, value.bind(py))?;
        }
        let types = py.import("types")?;
        let namespace = types.getattr("SimpleNamespace")?;
        namespace.call((), Some(&kwargs)).map(Bound::unbind)
    }
}

impl Clone for PyDynamicSolution {
    fn clone(&self) -> Self {
        Self {
            schema: Arc::clone(&self.schema),
            state: self.state.clone(),
            callback_view: self.callback_view.clone(),
            score: self.score,
            solver_config: self.solver_config.clone(),
        }
    }
}

unsafe impl Send for PyDynamicSolution {}
unsafe impl Sync for PyDynamicSolution {}

impl PlanningSolution for PyDynamicSolution {
    type Score = DynamicScore;

    fn score(&self) -> Option<Self::Score> {
        self.score
    }

    fn set_score(&mut self, score: Option<Self::Score>) {
        self.score = score;
    }

    fn update_entity_shadows(&mut self, descriptor_index: usize, entity_index: usize) {
        if self.entity_has_shadow_updates(descriptor_index) {
            if let Err(error) = self.refresh_entity_shadows(descriptor_index, entity_index) {
                panic!("dynamic shadow update failed: {error}");
            }
        }
    }

    fn update_all_shadows(&mut self) {
        if let Err(error) = self.refresh_all_shadows() {
            panic!("dynamic shadow update failed: {error}");
        }
    }

    fn is_initialized(&self) -> bool {
        self.state.is_initialized(&self.schema)
    }
}

impl DynamicModelBackend for PyDynamicSolution {
    type Score = DynamicScore;

    fn entity_count(&self, entity: EntityClassId) -> usize {
        self.state.entities.get(entity.0).map(Vec::len).unwrap_or(0)
    }

    fn get_scalar(&self, entity: EntityClassId, row: usize, variable: VariableId) -> Option<usize> {
        let name = self.variable_name(entity, variable)?;
        self.state.entities.get(entity.0)?.get(row)?.scalar(name)
    }

    fn set_scalar(
        &mut self,
        entity: EntityClassId,
        row: usize,
        variable: VariableId,
        value: Option<usize>,
    ) {
        let Some(name) = self.variable_name(entity, variable).map(str::to_string) else {
            return;
        };
        if let Some(entity_row) = self
            .state
            .entities
            .get_mut(entity.0)
            .and_then(|rows| rows.get_mut(row))
        {
            entity_row.set_scalar(&name, value);
        }
    }

    fn list_len(&self, entity: EntityClassId, row: usize, variable: VariableId) -> usize {
        let Some(name) = self.variable_name(entity, variable) else {
            return 0;
        };
        self.state
            .entities
            .get(entity.0)
            .and_then(|rows| rows.get(row))
            .and_then(|row| row.lists.get(name))
            .map(Vec::len)
            .unwrap_or(0)
    }

    fn list_get(
        &self,
        entity: EntityClassId,
        row: usize,
        variable: VariableId,
        pos: usize,
    ) -> Option<usize> {
        let name = self.variable_name(entity, variable)?;
        self.state
            .entities
            .get(entity.0)?
            .get(row)?
            .lists
            .get(name)?
            .get(pos)
            .copied()
    }

    fn list_insert(
        &mut self,
        entity: EntityClassId,
        row: usize,
        variable: VariableId,
        pos: usize,
        value: usize,
    ) {
        let Some(name) = self.variable_name(entity, variable).map(str::to_string) else {
            return;
        };
        if let Some(list) = self
            .state
            .entities
            .get_mut(entity.0)
            .and_then(|rows| rows.get_mut(row))
            .and_then(|row| row.lists.get_mut(&name))
        {
            list.insert(pos.min(list.len()), value);
        }
    }

    fn list_remove(
        &mut self,
        entity: EntityClassId,
        row: usize,
        variable: VariableId,
        pos: usize,
    ) -> Option<usize> {
        let name = self.variable_name(entity, variable)?.to_string();
        let list = self
            .state
            .entities
            .get_mut(entity.0)?
            .get_mut(row)?
            .lists
            .get_mut(&name)?;
        if pos < list.len() {
            Some(list.remove(pos))
        } else {
            None
        }
    }

    fn candidate_values(
        &self,
        entity: EntityClassId,
        row: usize,
        variable: VariableId,
    ) -> &[usize] {
        let Some(name) = self.variable_name(entity, variable) else {
            return &[];
        };
        self.state
            .entities
            .get(entity.0)
            .and_then(|rows| rows.get(row))
            .and_then(|row| row.candidates.get(name))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn list_element_count(&self, entity: EntityClassId, variable: VariableId) -> usize {
        let Some(name) = self.variable_name(entity, variable) else {
            return 0;
        };
        self.state
            .list_elements
            .get(entity.0)
            .and_then(|elements| elements.get(name))
            .map(Vec::len)
            .unwrap_or(0)
    }

    fn list_element(
        &self,
        entity: EntityClassId,
        variable: VariableId,
        element_index: usize,
    ) -> Option<usize> {
        let name = self.variable_name(entity, variable)?;
        self.state
            .list_elements
            .get(entity.0)?
            .get(name)?
            .get(element_index)
            .copied()
    }

    fn list_assigned_elements(&self, entity: EntityClassId, variable: VariableId) -> Vec<usize> {
        let Some(name) = self.variable_name(entity, variable) else {
            return Vec::new();
        };
        self.state
            .entities
            .get(entity.0)
            .into_iter()
            .flat_map(|rows| rows.iter())
            .filter_map(|row| row.lists.get(name))
            .flat_map(|values| values.iter().copied())
            .collect()
    }
}
