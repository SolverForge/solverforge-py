pub mod clone;
pub mod entity_table;
pub mod marshal;
pub mod variables;

use std::sync::Arc;

use solverforge_bridge::{DynamicModelBackend, EntityClassId, VariableId};
use solverforge_config::SolverConfig;
use solverforge_core::domain::PlanningSolution;

use crate::schema::DynamicSchema;
use crate::score::DynamicScore;

#[derive(Debug)]
pub struct PyDynamicSolution {
    pub schema: Arc<DynamicSchema>,
    pub state: entity_table::DynamicState,
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
}

impl Clone for PyDynamicSolution {
    fn clone(&self) -> Self {
        Self {
            schema: Arc::clone(&self.schema),
            state: self.state.clone(),
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
