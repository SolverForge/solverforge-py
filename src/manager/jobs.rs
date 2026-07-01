use std::collections::HashMap;

use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use solverforge_config::SolverConfig;
use solverforge_solver::{SolverEvent, SolverManager as UpstreamSolverManager, SolverManagerError};
use tokio::sync::mpsc;

use super::events::{event_to_python, status_to_python};
use crate::config::config_from_python;
use crate::error::py_err;
use crate::runtime::dynamic_runtime_model;
use crate::runtime::dynamic_scalar_search::validate_dynamic_runtime_bindings;
use crate::schema::build::solution_descriptor;
use crate::schema::{parse_schema, validate::validate_dynamic_schema};
use crate::score::DynamicScorePythonExt;
use crate::state::marshal::{export_solution, import_solution};
use crate::state::PyDynamicSolution;

#[derive(Debug, Clone)]
pub struct PyJobHandle {
    pub job_id: usize,
}

#[pyclass(name = "SolverManager")]
pub struct NativeSolverManager {
    manager: &'static UpstreamSolverManager<PyDynamicSolution>,
    receivers: Mutex<HashMap<usize, mpsc::UnboundedReceiver<SolverEvent<PyDynamicSolution>>>>,
    originals: Mutex<HashMap<usize, Py<PyAny>>>,
    score_families: Mutex<HashMap<usize, String>>,
    config: SolverConfig,
}

#[pymethods]
impl NativeSolverManager {
    #[new]
    fn new(config: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        Ok(Self {
            manager: Box::leak(Box::new(UpstreamSolverManager::new())),
            receivers: Mutex::new(HashMap::new()),
            originals: Mutex::new(HashMap::new()),
            score_families: Mutex::new(HashMap::new()),
            config: config_from_python(config)?,
        })
    }

    fn solve(
        &self,
        py: Python<'_>,
        solution: Py<PyAny>,
        schema: &Bound<'_, PyDict>,
    ) -> PyResult<usize> {
        let parsed = std::sync::Arc::new(parse_schema(schema)?);
        validate_dynamic_schema(&parsed)?;
        let descriptor = solution_descriptor(&parsed);
        let model = dynamic_runtime_model(&parsed, &descriptor)
            .map_err(|err| py_err(format!("failed to resolve dynamic runtime model: {err}")))?;
        validate_dynamic_runtime_bindings(&self.config, &parsed, &model)?;
        let copy_module = py.import("copy")?;
        let working_solution = copy_module
            .getattr("deepcopy")?
            .call1((solution.bind(py),))?
            .unbind();
        let mut dynamic_solution = import_solution(working_solution.bind(py), parsed)?;
        dynamic_solution.solver_config = self.config.clone();
        let score_family = dynamic_solution.schema.score_family.clone();
        let (job_id, receiver) = self
            .manager
            .solve(dynamic_solution)
            .map_err(manager_err_to_py)?;
        self.receivers.lock().insert(job_id, receiver);
        self.originals.lock().insert(job_id, solution.clone_ref(py));
        self.score_families.lock().insert(job_id, score_family);
        Ok(job_id)
    }

    fn get_status(&self, py: Python<'_>, job_id: usize) -> PyResult<Py<PyAny>> {
        let status = self.manager.get_status(job_id).map_err(manager_err_to_py)?;
        let score_family = self.score_family(job_id);
        status_to_python(py, status, score_family.as_str())
    }

    fn drain_events(&self, py: Python<'_>, job_id: usize) -> PyResult<Vec<Py<PyAny>>> {
        let mut receivers = self.receivers.lock();
        let receiver = receivers
            .get_mut(&job_id)
            .ok_or_else(|| py_err(format!("job {job_id} has no event receiver")))?;
        let mut events = Vec::new();
        loop {
            match receiver.try_recv() {
                Ok(event) => {
                    let score_family = self.score_family(job_id);
                    events.push(event_to_python(py, event, score_family.as_str())?);
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
        Ok(events)
    }

    fn snapshot(
        &self,
        py: Python<'_>,
        job_id: usize,
        snapshot_revision: Option<u64>,
    ) -> PyResult<Py<PyAny>> {
        let snapshot = self
            .manager
            .get_snapshot(job_id, snapshot_revision)
            .map_err(manager_err_to_py)?;
        let original = self
            .originals
            .lock()
            .get(&job_id)
            .map(|value| value.clone_ref(py))
            .ok_or_else(|| py_err(format!("job {job_id} has no original Python solution")))?;
        let copy_module = py.import("copy")?;
        let cloned = copy_module
            .getattr("deepcopy")?
            .call1((original.bind(py),))?
            .unbind();
        export_solution(cloned.bind(py), &snapshot.solution)?;
        if let Some(score) = snapshot.best_score {
            let score_family = self.score_family(job_id);
            cloned.bind(py).setattr(
                "score",
                score.to_python_for_family(py, score_family.as_str())?,
            )?;
        }
        Ok(cloned)
    }

    fn pause(&self, job_id: usize) -> PyResult<()> {
        self.manager.pause(job_id).map_err(manager_err_to_py)
    }

    fn resume(&self, job_id: usize) -> PyResult<()> {
        self.manager.resume(job_id).map_err(manager_err_to_py)
    }

    fn cancel(&self, job_id: usize) -> PyResult<()> {
        self.manager.cancel(job_id).map_err(manager_err_to_py)
    }

    fn delete(&self, job_id: usize) -> PyResult<()> {
        self.manager.delete(job_id).map_err(manager_err_to_py)?;
        self.receivers.lock().remove(&job_id);
        self.originals.lock().remove(&job_id);
        self.score_families.lock().remove(&job_id);
        Ok(())
    }
}

impl NativeSolverManager {
    fn score_family(&self, job_id: usize) -> String {
        self.score_families
            .lock()
            .get(&job_id)
            .cloned()
            .unwrap_or_else(|| "hard_medium_soft".to_string())
    }
}

fn manager_err_to_py(error: SolverManagerError) -> PyErr {
    py_err(error.to_string())
}
