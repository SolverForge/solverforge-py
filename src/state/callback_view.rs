use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::state::PyDynamicSolution;

#[derive(Debug, Default)]
pub struct CallbackRootContext {
    fields: BTreeMap<String, Py<PyAny>>,
}

impl CallbackRootContext {
    pub fn new(fields: BTreeMap<String, Py<PyAny>>) -> Self {
        Self { fields }
    }
}

#[derive(Debug)]
struct AttachedPythonObjects {
    python_solution: Py<PyAny>,
    entity_objects: Vec<Vec<Py<PyAny>>>,
    fact_objects: Vec<Vec<Py<PyAny>>>,
}

#[derive(Debug)]
struct ProjectedPythonObjects {
    python_solution: Option<Py<PyAny>>,
    entity_objects: Vec<Vec<Option<Py<PyAny>>>>,
    fact_objects: Vec<Vec<Option<Py<PyAny>>>>,
}

impl ProjectedPythonObjects {
    fn new(attached: &AttachedPythonObjects) -> Self {
        Self {
            python_solution: None,
            entity_objects: attached
                .entity_objects
                .iter()
                .map(|rows| (0..rows.len()).map(|_| None).collect())
                .collect(),
            fact_objects: attached
                .fact_objects
                .iter()
                .map(|rows| (0..rows.len()).map(|_| None).collect())
                .collect(),
        }
    }
}

#[derive(Debug)]
struct DetachedSolutionCache {
    revision: u64,
    python_solution: Py<PyAny>,
}

#[derive(Debug, Default)]
struct AttachedSyncState {
    full_solution_synced: bool,
    dirty_entities: BTreeSet<(usize, usize)>,
    synced_entities: BTreeSet<(usize, usize)>,
}

#[derive(Debug)]
pub struct PythonCallbackView {
    root_context: Arc<CallbackRootContext>,
    attached: Option<Arc<AttachedPythonObjects>>,
    projected: Option<Arc<Mutex<ProjectedPythonObjects>>>,
    attached_sync: Arc<Mutex<AttachedSyncState>>,
    detached_cache: Mutex<Option<DetachedSolutionCache>>,
}

impl Default for PythonCallbackView {
    fn default() -> Self {
        Self {
            root_context: Arc::new(CallbackRootContext::default()),
            attached: None,
            projected: None,
            attached_sync: Arc::new(Mutex::new(AttachedSyncState::default())),
            detached_cache: Mutex::new(None),
        }
    }
}

impl Clone for PythonCallbackView {
    fn clone(&self) -> Self {
        let attached = self.attached.as_ref().map(Arc::clone);
        Self {
            root_context: Arc::clone(&self.root_context),
            projected: attached
                .as_ref()
                .map(|attached| Arc::new(Mutex::new(ProjectedPythonObjects::new(attached)))),
            attached,
            attached_sync: Arc::new(Mutex::new(AttachedSyncState::default())),
            detached_cache: Mutex::new(None),
        }
    }
}

impl PythonCallbackView {
    pub fn from_import(
        python_solution: Py<PyAny>,
        entity_objects: Vec<Vec<Py<PyAny>>>,
        fact_objects: Vec<Vec<Py<PyAny>>>,
        root_fields: BTreeMap<String, Py<PyAny>>,
    ) -> Self {
        Self {
            root_context: Arc::new(CallbackRootContext::new(root_fields)),
            attached: Some(Arc::new(AttachedPythonObjects {
                python_solution,
                entity_objects,
                fact_objects,
            })),
            projected: None,
            attached_sync: Arc::new(Mutex::new(AttachedSyncState::default())),
            detached_cache: Mutex::new(None),
        }
    }

    pub fn mark_entity_dirty(&self, entity_index: usize, row_index: usize) {
        let mut state = self
            .attached_sync
            .lock()
            .expect("dynamic callback attached sync mutex poisoned");
        state.dirty_entities.insert((entity_index, row_index));
    }

    pub fn solution_view(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<Py<PyAny>> {
        if self.attached.is_some() {
            let python_solution = self
                .callback_solution_object(py, solution)?
                .expect("attached callback view must expose its solution object");
            self.sync_solution_if_needed(py, solution)?;
            return Ok(python_solution);
        }
        self.cached_detached_solution(py, solution)
    }

    pub fn unsynced_solution_view(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<Py<PyAny>> {
        if self.attached.is_some() {
            return Ok(self
                .callback_solution_object(py, solution)?
                .expect("attached callback view must expose its solution object"));
        }
        self.cached_detached_solution(py, solution)
    }

    pub fn entity_view(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
        entity_index: usize,
        row_index: usize,
    ) -> PyResult<Py<PyAny>> {
        if let Some(row) =
            self.sync_entity_object_if_needed(py, solution, entity_index, row_index)?
        {
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

    fn callback_solution_object(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<Option<Py<PyAny>>> {
        let Some(attached) = self.attached.as_ref() else {
            return Ok(None);
        };
        let Some(projected) = self.projected.as_ref() else {
            return Ok(Some(attached.python_solution.clone_ref(py)));
        };
        if let Some(python_solution) = projected
            .lock()
            .expect("dynamic callback projection mutex poisoned")
            .python_solution
            .as_ref()
        {
            return Ok(Some(python_solution.clone_ref(py)));
        }

        let entity_objects = attached
            .entity_objects
            .iter()
            .enumerate()
            .map(|(entity_index, rows)| {
                (0..rows.len())
                    .map(|row_index| {
                        self.callback_entity_object(py, entity_index, row_index)?
                            .ok_or_else(|| {
                                crate::error::py_err("missing projected callback entity row")
                            })
                    })
                    .collect::<PyResult<Vec<_>>>()
            })
            .collect::<PyResult<Vec<_>>>()?;
        let fact_objects = attached
            .fact_objects
            .iter()
            .enumerate()
            .map(|(fact_index, rows)| {
                (0..rows.len())
                    .map(|row_index| {
                        self.callback_fact_object(py, fact_index, row_index)?
                            .ok_or_else(|| {
                                crate::error::py_err("missing projected callback fact row")
                            })
                    })
                    .collect::<PyResult<Vec<_>>>()
            })
            .collect::<PyResult<Vec<_>>>()?;
        let python_solution = shallow_copy(py, &attached.python_solution)?;
        let python_solution_bound = python_solution.bind(py);
        for (entity, rows) in solution.schema().entities.iter().zip(&entity_objects) {
            replace_collection(py, python_solution_bound, &entity.collection, rows)?;
        }
        for (fact, rows) in solution.schema().facts.iter().zip(&fact_objects) {
            replace_collection(py, python_solution_bound, &fact.collection, rows)?;
        }

        let mut cache = projected
            .lock()
            .expect("dynamic callback projection mutex poisoned");
        if cache.python_solution.is_none() {
            cache.python_solution = Some(python_solution);
        }
        Ok(cache
            .python_solution
            .as_ref()
            .map(|solution| solution.clone_ref(py)))
    }

    fn callback_entity_object(
        &self,
        py: Python<'_>,
        entity_index: usize,
        row_index: usize,
    ) -> PyResult<Option<Py<PyAny>>> {
        let Some(attached) = self.attached.as_ref() else {
            return Ok(None);
        };
        let Some(template) = attached
            .entity_objects
            .get(entity_index)
            .and_then(|rows| rows.get(row_index))
        else {
            return Ok(None);
        };
        let Some(projected) = self.projected.as_ref() else {
            return Ok(Some(template.clone_ref(py)));
        };
        if let Some(object) = projected
            .lock()
            .expect("dynamic callback projection mutex poisoned")
            .entity_objects
            .get(entity_index)
            .and_then(|rows| rows.get(row_index))
            .and_then(Option::as_ref)
        {
            return Ok(Some(object.clone_ref(py)));
        }

        let object = shallow_copy(py, template)?;
        let mut cache = projected
            .lock()
            .expect("dynamic callback projection mutex poisoned");
        let slot = cache
            .entity_objects
            .get_mut(entity_index)
            .and_then(|rows| rows.get_mut(row_index))
            .expect("callback projection shape must match attached entities");
        if slot.is_none() {
            *slot = Some(object);
        }
        Ok(slot.as_ref().map(|object| object.clone_ref(py)))
    }

    fn callback_fact_object(
        &self,
        py: Python<'_>,
        fact_index: usize,
        row_index: usize,
    ) -> PyResult<Option<Py<PyAny>>> {
        let Some(attached) = self.attached.as_ref() else {
            return Ok(None);
        };
        let Some(template) = attached
            .fact_objects
            .get(fact_index)
            .and_then(|rows| rows.get(row_index))
        else {
            return Ok(None);
        };
        let Some(projected) = self.projected.as_ref() else {
            return Ok(Some(template.clone_ref(py)));
        };
        if let Some(object) = projected
            .lock()
            .expect("dynamic callback projection mutex poisoned")
            .fact_objects
            .get(fact_index)
            .and_then(|rows| rows.get(row_index))
            .and_then(Option::as_ref)
        {
            return Ok(Some(object.clone_ref(py)));
        }

        let object = shallow_copy(py, template)?;
        let mut cache = projected
            .lock()
            .expect("dynamic callback projection mutex poisoned");
        let slot = cache
            .fact_objects
            .get_mut(fact_index)
            .and_then(|rows| rows.get_mut(row_index))
            .expect("callback projection shape must match attached facts");
        if slot.is_none() {
            *slot = Some(object);
        }
        Ok(slot.as_ref().map(|object| object.clone_ref(py)))
    }

    fn materialize_detached_solution(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<Py<PyAny>> {
        let kwargs = PyDict::new(py);
        let types = py.import("types")?;
        let namespace = types.getattr("SimpleNamespace")?;
        for (name, value) in &self.root_context.fields {
            kwargs.set_item(name.as_str(), value.bind(py))?;
        }
        for (name, value) in solution.state.solution_fields.iter() {
            let value = value.to_python(py)?;
            kwargs.set_item(name.as_str(), value.bind(py))?;
        }
        for (entity_index, entity_schema) in solution.schema().entities.iter().enumerate() {
            let mut rows = Vec::new();
            if let Some(entity_rows) = solution.state.entities.get(entity_index) {
                for (row_index, row) in entity_rows.iter().enumerate() {
                    rows.push(solution.entity_row_snapshot_with_namespace(
                        py,
                        &namespace,
                        entity_index,
                        row_index,
                        row,
                    )?);
                }
            }
            kwargs.set_item(entity_schema.collection.as_str(), PyList::new(py, rows)?)?;
        }
        for (fact_index, fact_schema) in solution.schema().facts.iter().enumerate() {
            let mut rows = Vec::new();
            if let Some(fact_rows) = solution.state.facts.get(fact_index) {
                for (row_index, row) in fact_rows.iter().enumerate() {
                    rows.push(solution.fact_row_snapshot_with_namespace(
                        py,
                        &namespace,
                        fact_schema.type_name.as_str(),
                        row_index,
                        row,
                    )?);
                }
            }
            kwargs.set_item(fact_schema.collection.as_str(), PyList::new(py, rows)?)?;
        }
        namespace.call((), Some(&kwargs)).map(Bound::unbind)
    }

    fn cached_detached_solution(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<Py<PyAny>> {
        let mut cache = self
            .detached_cache
            .lock()
            .expect("dynamic callback detached cache mutex poisoned");
        if let Some(cache) = cache.as_ref() {
            if cache.revision == solution.revision() {
                return Ok(cache.python_solution.clone_ref(py));
            }
        }
        let python_solution = self.materialize_detached_solution(py, solution)?;
        *cache = Some(DetachedSolutionCache {
            revision: solution.revision(),
            python_solution: python_solution.clone_ref(py),
        });
        Ok(python_solution)
    }

    fn sync_solution_if_needed(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<()> {
        let sync_all = {
            let state = self
                .attached_sync
                .lock()
                .expect("dynamic callback attached sync mutex poisoned");
            !state.full_solution_synced
        };
        if sync_all {
            self.sync_all_python_objects(py, solution)?;
            let mut state = self
                .attached_sync
                .lock()
                .expect("dynamic callback attached sync mutex poisoned");
            state.full_solution_synced = true;
            state.dirty_entities.clear();
            state.synced_entities.clear();
            for (entity_index, rows) in solution.state.entities.iter().enumerate() {
                for row_index in 0..rows.len() {
                    state.synced_entities.insert((entity_index, row_index));
                }
            }
            return Ok(());
        }

        let dirty_entities = {
            let mut state = self
                .attached_sync
                .lock()
                .expect("dynamic callback attached sync mutex poisoned");
            std::mem::take(&mut state.dirty_entities)
        };
        for (entity_index, row_index) in dirty_entities {
            let _ = self.sync_entity_object_now(py, solution, entity_index, row_index)?;
            let mut state = self
                .attached_sync
                .lock()
                .expect("dynamic callback attached sync mutex poisoned");
            state.synced_entities.insert((entity_index, row_index));
        }
        Ok(())
    }

    fn sync_all_python_objects(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<()> {
        for entity_index in 0..solution.state.entities.len() {
            for row_index in 0..solution.state.entities[entity_index].len() {
                let _ = self.sync_entity_object_now(py, solution, entity_index, row_index)?;
            }
        }
        for fact_index in 0..solution.state.facts.len() {
            for row_index in 0..solution.state.facts[fact_index].len() {
                self.sync_fact_object(py, solution, fact_index, row_index)?;
            }
        }
        Ok(())
    }

    fn sync_entity_object_if_needed(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
        entity_index: usize,
        row_index: usize,
    ) -> PyResult<Option<Py<PyAny>>> {
        if self.attached.is_none() {
            return Ok(None);
        }
        let needs_sync = {
            let state = self
                .attached_sync
                .lock()
                .expect("dynamic callback attached sync mutex poisoned");
            state.dirty_entities.contains(&(entity_index, row_index))
                || !state.synced_entities.contains(&(entity_index, row_index))
        };
        if needs_sync {
            let row = self.sync_entity_object_now(py, solution, entity_index, row_index)?;
            let mut state = self
                .attached_sync
                .lock()
                .expect("dynamic callback attached sync mutex poisoned");
            state.dirty_entities.remove(&(entity_index, row_index));
            state.synced_entities.insert((entity_index, row_index));
            return Ok(row);
        }
        self.callback_entity_object(py, entity_index, row_index)
    }

    fn sync_entity_object_now(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
        entity_index: usize,
        row_index: usize,
    ) -> PyResult<Option<Py<PyAny>>> {
        let Some(object) = self.callback_entity_object(py, entity_index, row_index)? else {
            return Ok(None);
        };
        let Some(entity_schema) = solution.schema().entities.get(entity_index) else {
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
        for (variable_index, variable) in entity_schema.variables.iter().enumerate() {
            let attribute_name = if object_bound
                .get_type()
                .getattr(variable.name.as_str())
                .is_ok()
            {
                variable.storage_name.as_str()
            } else {
                variable.name.as_str()
            };
            match variable.kind.as_str() {
                "planning_variable" => {
                    if let Some(value) = row.scalar_at(variable_index) {
                        object_bound.setattr(attribute_name, value)?;
                    } else {
                        object_bound.setattr(attribute_name, py.None())?;
                    }
                }
                "planning_list_variable" => {
                    let values = row
                        .list_at(variable_index)
                        .map(<[usize]>::to_vec)
                        .unwrap_or_default();
                    object_bound.setattr(attribute_name, values)?;
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
        Ok(Some(object))
    }

    fn sync_fact_object(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
        fact_index: usize,
        row_index: usize,
    ) -> PyResult<()> {
        let Some(object) = self.callback_fact_object(py, fact_index, row_index)? else {
            return Ok(());
        };
        let Some(fact_schema) = solution.schema().facts.get(fact_index) else {
            return Ok(());
        };
        let object_bound = object.bind(py);
        object_bound.setattr("_solverforge_fact_index", row_index)?;
        object_bound.setattr("_solverforge_fact_class", fact_schema.type_name.as_str())?;
        Ok(())
    }
}

fn shallow_copy(py: Python<'_>, object: &Py<PyAny>) -> PyResult<Py<PyAny>> {
    let copied = py
        .import("copy")?
        .getattr("copy")?
        .call1((object.bind(py),))
        .map(Bound::unbind)?;
    if copied.bind(py).is(object.bind(py)) {
        return Err(crate::error::py_err(
            "callback projection requires __copy__ to return an independent object",
        ));
    }
    Ok(copied)
}

fn replace_collection(
    py: Python<'_>,
    python_solution: &Bound<'_, PyAny>,
    collection_name: &str,
    rows: &[Py<PyAny>],
) -> PyResult<()> {
    python_solution.setattr(collection_name, PyList::new(py, rows)?)
}
