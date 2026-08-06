use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use solverforge_bridge::{DynamicModelBackend, EntityClassId, VariableId};
use solverforge_config::SolverConfig;
use solverforge_core::domain::PlanningSolution;

use crate::error::panic_with_py_err;
use crate::schema::runtime_plan::CompiledRuntimePlan;
use crate::schema::DynamicSchema;
use crate::score::DynamicScore;
use crate::state::callback_view::PythonCallbackView;
use crate::state::entity_table::DynamicState;
use crate::value::DynamicValue;

#[derive(Debug)]
pub struct PyDynamicSolution {
    runtime_plan: Arc<CompiledRuntimePlan>,
    pub state: DynamicState,
    pub callback_view: PythonCallbackView,
    pub score: Option<DynamicScore>,
    pub solver_config: SolverConfig,
    pub revision: u64,
}

impl PyDynamicSolution {
    /// Construct a dynamic solution from one compiled runtime plan.
    ///
    /// The plan is intentionally atomic: callers cannot combine a schema,
    /// descriptor, and runtime model from different compilations.
    pub fn from_runtime_plan(
        runtime_plan: Arc<CompiledRuntimePlan>,
        state: DynamicState,
        callback_view: PythonCallbackView,
        score: Option<DynamicScore>,
        solver_config: SolverConfig,
        revision: u64,
    ) -> Self {
        Self {
            runtime_plan,
            state,
            callback_view,
            score,
            solver_config,
            revision,
        }
    }

    pub(crate) fn schema(&self) -> &DynamicSchema {
        self.runtime_plan.schema()
    }

    pub(crate) fn runtime_plan(&self) -> &Arc<CompiledRuntimePlan> {
        &self.runtime_plan
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn mark_entity_dirty(&self, descriptor_index: usize, entity_index: usize) {
        self.callback_view
            .mark_entity_dirty(descriptor_index, entity_index);
    }

    fn record_entity_change(&mut self, descriptor_index: usize, entity_index: usize) {
        self.bump_revision();
        self.mark_entity_dirty(descriptor_index, entity_index);
    }

    pub(crate) fn set_scalar_value(
        &mut self,
        descriptor_index: usize,
        entity_index: usize,
        variable_index: usize,
        value: Option<usize>,
    ) {
        if let Some(row) = self
            .state
            .entities
            .get_mut(descriptor_index)
            .and_then(|rows| rows.get_mut(entity_index))
        {
            if row.scalar_at(variable_index) != value {
                row.set_scalar_at(variable_index, value);
                self.record_entity_change(descriptor_index, entity_index);
            }
        }
    }

    /// Replaces an already-imported list row through one logical state
    /// transition.  Dynamic core list access uses this strict primitive for
    /// route replacement; it never composes a replacement from removes and
    /// inserts, which would publish multiple revisions/dirty-row marks.
    ///
    /// `false` means the requested row/list does not exist. A valid no-op
    /// replacement returns `true` without manufacturing a revision.
    pub(crate) fn replace_existing_list_value(
        &mut self,
        descriptor_index: usize,
        entity_index: usize,
        variable_index: usize,
        values: Vec<usize>,
    ) -> bool {
        let mut changed = false;
        let valid = if let Some(list) = self
            .state
            .entities
            .get_mut(descriptor_index)
            .and_then(|rows| rows.get_mut(entity_index))
            .and_then(|row| row.list_mut_at(variable_index))
        {
            if list.as_slice() != values.as_slice() {
                *list = values;
                changed = true;
            }
            true
        } else {
            false
        };
        if changed {
            self.record_entity_change(descriptor_index, entity_index);
        }
        valid
    }

    /// Sets one existing list position without clamping a core-owned index.
    /// Invalid positions are a failed access contract, not an invitation to
    /// silently write a different position.
    pub(crate) fn set_existing_list_value(
        &mut self,
        descriptor_index: usize,
        entity_index: usize,
        variable_index: usize,
        position: usize,
        value: usize,
    ) -> bool {
        let mut changed = false;
        let valid = if let Some(list) = self
            .state
            .entities
            .get_mut(descriptor_index)
            .and_then(|rows| rows.get_mut(entity_index))
            .and_then(|row| row.list_mut_at(variable_index))
        {
            let Some(current) = list.get_mut(position) else {
                return false;
            };
            if *current != value {
                *current = value;
                changed = true;
            }
            true
        } else {
            false
        };
        if changed {
            self.record_entity_change(descriptor_index, entity_index);
        }
        valid
    }

    /// Inserts one value at a validated position. Unlike detached Python state,
    /// adapter, this never clamps an out-of-range position to the list end.
    pub(crate) fn insert_existing_list_value(
        &mut self,
        descriptor_index: usize,
        entity_index: usize,
        variable_index: usize,
        position: usize,
        value: usize,
    ) -> bool {
        let valid = if let Some(list) = self
            .state
            .entities
            .get_mut(descriptor_index)
            .and_then(|rows| rows.get_mut(entity_index))
            .and_then(|row| row.list_mut_at(variable_index))
        {
            if position > list.len() {
                return false;
            }
            list.insert(position, value);
            true
        } else {
            false
        };
        if valid {
            self.record_entity_change(descriptor_index, entity_index);
        }
        valid
    }

    /// Reverses one validated half-open range as a single logical mutation.
    pub(crate) fn reverse_existing_list_range(
        &mut self,
        descriptor_index: usize,
        entity_index: usize,
        variable_index: usize,
        start: usize,
        end: usize,
    ) -> bool {
        let mut changed = false;
        let valid = if let Some(list) = self
            .state
            .entities
            .get_mut(descriptor_index)
            .and_then(|rows| rows.get_mut(entity_index))
            .and_then(|row| row.list_mut_at(variable_index))
        {
            if start > end || end > list.len() {
                return false;
            }
            if end.saturating_sub(start) > 1 {
                list[start..end].reverse();
                changed = true;
            }
            true
        } else {
            false
        };
        if changed {
            self.record_entity_change(descriptor_index, entity_index);
        }
        valid
    }

    /// Removes one validated half-open range as a single logical mutation.
    pub(crate) fn remove_existing_list_range(
        &mut self,
        descriptor_index: usize,
        entity_index: usize,
        variable_index: usize,
        start: usize,
        end: usize,
    ) -> Option<Vec<usize>> {
        let removed = {
            let list = self
                .state
                .entities
                .get_mut(descriptor_index)?
                .get_mut(entity_index)?
                .list_mut_at(variable_index)?;
            if start > end || end > list.len() {
                return None;
            }
            list.drain(start..end).collect::<Vec<_>>()
        };
        if !removed.is_empty() {
            self.record_entity_change(descriptor_index, entity_index);
        }
        Some(removed)
    }

    /// Inserts a range at one validated position as a single logical
    /// mutation. An empty range is a valid no-op and does not publish a fake
    /// revision.
    pub(crate) fn insert_existing_list_range(
        &mut self,
        descriptor_index: usize,
        entity_index: usize,
        variable_index: usize,
        position: usize,
        values: Vec<usize>,
    ) -> bool {
        let changed = !values.is_empty();
        let valid = if let Some(list) = self
            .state
            .entities
            .get_mut(descriptor_index)
            .and_then(|rows| rows.get_mut(entity_index))
            .and_then(|row| row.list_mut_at(variable_index))
        {
            if position > list.len() {
                return false;
            }
            if changed {
                list.splice(position..position, values);
            }
            true
        } else {
            false
        };
        if changed && valid {
            self.record_entity_change(descriptor_index, entity_index);
        }
        valid
    }

    pub(crate) fn insert_list_value(
        &mut self,
        descriptor_index: usize,
        entity_index: usize,
        variable_index: usize,
        pos: usize,
        value: usize,
    ) {
        let mut changed = false;
        if let Some(list) = self
            .state
            .entities
            .get_mut(descriptor_index)
            .and_then(|rows| rows.get_mut(entity_index))
            .and_then(|row| row.list_mut_at(variable_index))
        {
            list.insert(pos.min(list.len()), value);
            changed = true;
        }
        if changed {
            self.record_entity_change(descriptor_index, entity_index);
        }
    }

    pub(crate) fn remove_list_value(
        &mut self,
        descriptor_index: usize,
        entity_index: usize,
        variable_index: usize,
        pos: usize,
    ) -> Option<usize> {
        let removed = self
            .state
            .entities
            .get_mut(descriptor_index)?
            .get_mut(entity_index)?
            .list_mut_at(variable_index)
            .and_then(|list| (pos < list.len()).then(|| list.remove(pos)));
        if removed.is_some() {
            self.record_entity_change(descriptor_index, entity_index);
        }
        removed
    }

    pub fn entity_count(&self, descriptor_index: usize) -> usize {
        self.state
            .entities
            .get(descriptor_index)
            .map(Vec::len)
            .unwrap_or(0)
    }

    pub fn refresh_all_shadows(&mut self) -> PyResult<()> {
        let schema = self.schema();
        let targets = schema
            .entities
            .iter()
            .enumerate()
            .filter_map(|(descriptor_index, entity)| {
                schema
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
        let Some(entity_schema) = self.schema().entities.get(descriptor_index) else {
            return false;
        };
        self.schema()
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
        let Some(entity_schema) = self.schema().entities.get(descriptor_index) else {
            return Ok(());
        };
        let collection = entity_schema.collection.clone();
        Python::attach(|py| -> PyResult<()> {
            let callbacks = self
                .schema()
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
                let updates = updates
                    .iter()
                    .map(|item| {
                        let (key, value) = item;
                        Ok((key.extract::<String>()?, DynamicValue::from_python(&value)?))
                    })
                    .collect::<PyResult<Vec<_>>>()?;
                let mut changed = false;
                if let Some(row) = self
                    .state
                    .entities
                    .get_mut(descriptor_index)
                    .and_then(|rows| rows.get_mut(entity_index))
                {
                    for (key, value) in updates {
                        if row.fields.get(&key) != Some(&value) {
                            row.set_field(key.clone(), value);
                            changed = true;
                        }
                        row.shadow_fields.insert(key);
                    }
                }
                if changed {
                    self.record_entity_change(descriptor_index, entity_index);
                }
            }
            Ok(())
        })
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
        let namespace = simple_namespace_type(py)?;
        self.entity_row_snapshot_with_namespace(py, &namespace, entity_index, row_index, row)
    }

    pub(crate) fn entity_row_snapshot_with_namespace(
        &self,
        py: Python<'_>,
        namespace: &Bound<'_, PyAny>,
        entity_index: usize,
        row_index: usize,
        row: &crate::state::entity_table::DynamicEntityRow,
    ) -> PyResult<Py<PyAny>> {
        let kwargs = PyDict::new(py);
        kwargs.set_item("_solverforge_entity_index", row_index)?;
        kwargs.set_item("_solverforge_descriptor_index", entity_index)?;
        kwargs.set_item(
            "_solverforge_entity_class",
            self.schema().entities[entity_index].type_name.as_str(),
        )?;
        for (name, value) in row.fields.iter() {
            let value = value.to_python(py)?;
            kwargs.set_item(name, value.bind(py))?;
        }
        for (variable_index, variable) in self.schema().entities[entity_index]
            .variables
            .iter()
            .enumerate()
        {
            match variable.kind.as_str() {
                "planning_variable" => {
                    if let Some(value) = row.scalar_at(variable_index) {
                        kwargs.set_item(variable.name.as_str(), value)?;
                    } else {
                        kwargs.set_item(variable.name.as_str(), py.None())?;
                    }
                }
                "planning_list_variable" => {
                    let values = row
                        .list_at(variable_index)
                        .map(<[usize]>::to_vec)
                        .unwrap_or_default();
                    kwargs.set_item(variable.name.as_str(), values)?;
                }
                _ => {}
            }
        }
        namespace.call((), Some(&kwargs)).map(Bound::unbind)
    }

    pub(crate) fn fact_row_snapshot_with_namespace(
        &self,
        py: Python<'_>,
        namespace: &Bound<'_, PyAny>,
        fact_type_name: &str,
        row_index: usize,
        row: &crate::state::entity_table::DynamicEntityRow,
    ) -> PyResult<Py<PyAny>> {
        let kwargs = PyDict::new(py);
        kwargs.set_item("_solverforge_fact_index", row_index)?;
        kwargs.set_item("_solverforge_fact_class", fact_type_name)?;
        for (name, value) in row.fields.iter() {
            let value = value.to_python(py)?;
            kwargs.set_item(name, value.bind(py))?;
        }
        namespace.call((), Some(&kwargs)).map(Bound::unbind)
    }
}

fn simple_namespace_type(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    py.import("types")?.getattr("SimpleNamespace")
}

impl Clone for PyDynamicSolution {
    fn clone(&self) -> Self {
        Self {
            runtime_plan: Arc::clone(&self.runtime_plan),
            state: self.state.clone(),
            callback_view: self.callback_view.clone(),
            score: self.score,
            solver_config: self.solver_config.clone(),
            revision: self.revision,
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
                panic_with_py_err::<()>(error);
            }
        }
    }

    fn update_all_shadows(&mut self) {
        if let Err(error) = self.refresh_all_shadows() {
            panic_with_py_err::<()>(error);
        }
    }

    fn is_initialized(&self) -> bool {
        self.state.is_initialized(self.schema())
    }
}

impl DynamicModelBackend for PyDynamicSolution {
    type Score = DynamicScore;

    fn entity_count(&self, entity: EntityClassId) -> usize {
        self.state.entities.get(entity.0).map(Vec::len).unwrap_or(0)
    }

    fn get_scalar(&self, entity: EntityClassId, row: usize, variable: VariableId) -> Option<usize> {
        self.state
            .entities
            .get(entity.0)?
            .get(row)?
            .scalar_at(variable.0)
    }

    fn set_scalar(
        &mut self,
        entity: EntityClassId,
        row: usize,
        variable: VariableId,
        value: Option<usize>,
    ) {
        self.set_scalar_value(entity.0, row, variable.0, value);
    }

    fn list_len(&self, entity: EntityClassId, row: usize, variable: VariableId) -> usize {
        self.state
            .entities
            .get(entity.0)
            .and_then(|rows| rows.get(row))
            .and_then(|row| row.list_at(variable.0))
            .map(<[usize]>::len)
            .unwrap_or(0)
    }

    fn list_get(
        &self,
        entity: EntityClassId,
        row: usize,
        variable: VariableId,
        pos: usize,
    ) -> Option<usize> {
        self.state
            .entities
            .get(entity.0)?
            .get(row)?
            .list_at(variable.0)?
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
        self.insert_list_value(entity.0, row, variable.0, pos, value);
    }

    fn list_remove(
        &mut self,
        entity: EntityClassId,
        row: usize,
        variable: VariableId,
        pos: usize,
    ) -> Option<usize> {
        self.remove_list_value(entity.0, row, variable.0, pos)
    }

    fn candidate_values(
        &self,
        entity: EntityClassId,
        row: usize,
        variable: VariableId,
    ) -> &[usize] {
        self.state
            .entities
            .get(entity.0)
            .and_then(|rows| rows.get(row))
            .and_then(|row| row.candidates_at(variable.0))
            .unwrap_or(&[])
    }

    fn scalar_value_is_legal(
        &self,
        entity: EntityClassId,
        row: usize,
        variable: VariableId,
        value: usize,
    ) -> bool {
        if let Some(candidates) = self
            .state
            .entities
            .get(entity.0)
            .and_then(|rows| rows.get(row))
            .and_then(|row| row.candidates_at(variable.0))
        {
            return candidates.contains(&value);
        }
        self.state
            .scalar_value_range_at(entity.0, variable.0)
            .is_some_and(|values| values.contains(&value))
    }

    fn list_element_count(&self, entity: EntityClassId, variable: VariableId) -> usize {
        self.state
            .list_elements_at(entity.0, variable.0)
            .map(<[usize]>::len)
            .unwrap_or(0)
    }

    fn list_element(
        &self,
        entity: EntityClassId,
        variable: VariableId,
        element_index: usize,
    ) -> Option<usize> {
        self.state
            .list_elements_at(entity.0, variable.0)?
            .get(element_index)
            .copied()
    }

    fn list_assigned_elements(&self, entity: EntityClassId, variable: VariableId) -> Vec<usize> {
        self.state
            .entities
            .get(entity.0)
            .into_iter()
            .flat_map(|rows| rows.iter())
            .filter_map(|row| row.list_at(variable.0))
            .flat_map(|values| values.iter().copied())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pyo3::prelude::*;
    use pyo3::types::PyList;

    use super::PyDynamicSolution;
    use crate::schema::runtime_plan::CompiledRuntimePlan;
    use crate::schema::types::ListMetadataSchema;
    use crate::schema::{DynamicSchema, EntitySchema, VariableSchema};
    use crate::state::callback_view::PythonCallbackView;
    use crate::state::entity_table::{DynamicEntityRow, DynamicState};

    fn list_solution(py: Python<'_>) -> PyDynamicSolution {
        let schema = Arc::new(DynamicSchema {
            solution_type: "ListPlan".to_string(),
            score_family: "soft".to_string(),
            entities: vec![EntitySchema {
                type_name: "Route".to_string(),
                collection: "routes".to_string(),
                variables: vec![VariableSchema {
                    name: "visits".to_string(),
                    storage_name: "__solverforge_visits".to_string(),
                    kind: "planning_list_variable".to_string(),
                    element_collection: Some("visit_values".to_string()),
                    list_metadata: Some(ListMetadataSchema::default()),
                    ..VariableSchema::default()
                }],
            }],
            facts: Vec::new(),
            constraints: py.None(),
            scalar_groups: PyList::empty(py).unbind().into_any(),
            assignment_scalar_groups: Vec::new(),
            conflict_repairs: PyList::empty(py).unbind().into_any(),
            candidate_metrics: PyList::empty(py).unbind().into_any(),
            shadow_updates: Vec::new(),
        });
        let runtime_plan = Arc::new(
            CompiledRuntimePlan::from_schema(schema)
                .expect("list test schema should compile into a runtime plan"),
        );
        let mut row = DynamicEntityRow::with_variable_count(1);
        row.set_list_at(0, vec![1, 2, 3]);
        PyDynamicSolution::from_runtime_plan(
            runtime_plan,
            DynamicState {
                entities: vec![vec![row]],
                ..DynamicState::default()
            },
            PythonCallbackView::default(),
            None,
            solverforge_config::SolverConfig::default(),
            0,
        )
    }

    #[test]
    fn strict_list_mutations_are_single_revision_operations() {
        Python::initialize();
        Python::attach(|py| {
            let mut solution = list_solution(py);
            let mut revision = solution.revision();

            assert!(solution.set_existing_list_value(0, 0, 0, 1, 9));
            revision += 1;
            assert_eq!(solution.revision(), revision);
            assert_eq!(
                solution.state.entities[0][0].list_at(0),
                Some(&[1, 9, 3][..])
            );

            assert!(solution.replace_existing_list_value(0, 0, 0, vec![4, 5, 6]));
            revision += 1;
            assert_eq!(solution.revision(), revision);
            assert_eq!(
                solution.state.entities[0][0].list_at(0),
                Some(&[4, 5, 6][..])
            );

            assert!(solution.reverse_existing_list_range(0, 0, 0, 0, 3));
            revision += 1;
            assert_eq!(solution.revision(), revision);
            assert_eq!(
                solution.state.entities[0][0].list_at(0),
                Some(&[6, 5, 4][..])
            );

            assert_eq!(
                solution.remove_existing_list_range(0, 0, 0, 1, 3),
                Some(vec![5, 4])
            );
            revision += 1;
            assert_eq!(solution.revision(), revision);
            assert_eq!(solution.state.entities[0][0].list_at(0), Some(&[6][..]));

            assert!(solution.insert_existing_list_range(0, 0, 0, 1, vec![7, 8]));
            revision += 1;
            assert_eq!(solution.revision(), revision);
            assert_eq!(
                solution.state.entities[0][0].list_at(0),
                Some(&[6, 7, 8][..])
            );

            assert!(solution.insert_existing_list_value(0, 0, 0, 3, 9));
            revision += 1;
            assert_eq!(solution.revision(), revision);
            assert_eq!(
                solution.state.entities[0][0].list_at(0),
                Some(&[6, 7, 8, 9][..])
            );

            assert!(solution.replace_existing_list_value(0, 0, 0, vec![6, 7, 8, 9]));
            assert!(solution.set_existing_list_value(0, 0, 0, 1, 7));
            assert!(solution.reverse_existing_list_range(0, 0, 0, 1, 2));
            assert_eq!(
                solution.remove_existing_list_range(0, 0, 0, 2, 2),
                Some(vec![])
            );
            assert!(solution.insert_existing_list_range(0, 0, 0, 2, Vec::new()));
            assert_eq!(solution.revision(), revision);

            assert!(!solution.set_existing_list_value(0, 0, 0, 4, 10));
            assert!(!solution.insert_existing_list_value(0, 0, 0, 5, 10));
            assert!(!solution.reverse_existing_list_range(0, 0, 0, 3, 2));
            assert_eq!(solution.remove_existing_list_range(0, 0, 0, 3, 5), None);
            assert!(!solution.insert_existing_list_range(0, 0, 0, 5, vec![10]));
            assert_eq!(solution.revision(), revision);
        });
    }
}
