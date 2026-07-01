use std::collections::BTreeMap;
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
struct DetachedSolutionCache {
    revision: u64,
    python_solution: Py<PyAny>,
}

#[derive(Debug)]
pub struct PythonCallbackView {
    root_context: Arc<CallbackRootContext>,
    attached: Option<AttachedPythonObjects>,
    detached_cache: Mutex<Option<DetachedSolutionCache>>,
}

impl Default for PythonCallbackView {
    fn default() -> Self {
        Self {
            root_context: Arc::new(CallbackRootContext::default()),
            attached: None,
            detached_cache: Mutex::new(None),
        }
    }
}

impl Clone for PythonCallbackView {
    fn clone(&self) -> Self {
        Self {
            root_context: Arc::clone(&self.root_context),
            attached: None,
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
            attached: Some(AttachedPythonObjects {
                python_solution,
                entity_objects,
                fact_objects,
            }),
            detached_cache: Mutex::new(None),
        }
    }

    pub fn solution_view(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<Py<PyAny>> {
        if let Some(attached) = self.attached.as_ref() {
            self.sync_all_python_objects(py, solution)?;
            return Ok(attached.python_solution.clone_ref(py));
        }
        self.cached_detached_solution(py, solution)
    }

    pub fn unsynced_solution_view(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<Py<PyAny>> {
        if let Some(attached) = self.attached.as_ref() {
            return Ok(attached.python_solution.clone_ref(py));
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

    fn materialize_detached_solution(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
    ) -> PyResult<Py<PyAny>> {
        let kwargs = PyDict::new(py);
        for (name, value) in &self.root_context.fields {
            kwargs.set_item(name.as_str(), value.bind(py))?;
        }
        for (name, value) in &solution.state.solution_fields {
            let value = value.to_python(py)?;
            kwargs.set_item(name.as_str(), value.bind(py))?;
        }
        for (entity_index, entity_schema) in solution.schema.entities.iter().enumerate() {
            let mut rows = Vec::new();
            if let Some(entity_rows) = solution.state.entities.get(entity_index) {
                for (row_index, row) in entity_rows.iter().enumerate() {
                    rows.push(solution.entity_row_snapshot(py, entity_index, row_index, row)?);
                }
            }
            kwargs.set_item(entity_schema.collection.as_str(), PyList::new(py, rows)?)?;
        }
        for (fact_index, fact_schema) in solution.schema.facts.iter().enumerate() {
            let mut rows = Vec::new();
            if let Some(fact_rows) = solution.state.facts.get(fact_index) {
                for (row_index, row) in fact_rows.iter().enumerate() {
                    rows.push(solution.fact_row_snapshot(
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
        let Some(attached) = self.attached.as_ref() else {
            return Ok(None);
        };
        let Some(object) = attached
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
        let Some(attached) = self.attached.as_ref() else {
            return Ok(());
        };
        let Some(object) = attached
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
