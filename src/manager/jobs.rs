use std::collections::HashMap;

use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use solverforge_config::SolverConfig;
use solverforge_solver::{SolverEvent, SolverManager as UpstreamSolverManager, SolverManagerError};
use tokio::sync::mpsc;

use super::events::{event_to_python, status_to_python, telemetry_detail_to_python};
use super::PyQualifiedCandidateTraceProvenance;
use crate::config::config_from_python;
use crate::error::py_err;
use crate::runtime::dynamic_assignment_group::validate_assignment_construction_groups;
use crate::schema::compiled::CompiledSchema;
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

    #[pyo3(signature = (solution, schema, *, qualified_candidate_trace_provenance = None))]
    fn solve(
        &self,
        py: Python<'_>,
        solution: Py<PyAny>,
        schema: PyRef<'_, CompiledSchema>,
        qualified_candidate_trace_provenance: Option<
            PyRef<'_, PyQualifiedCandidateTraceProvenance>,
        >,
    ) -> PyResult<usize> {
        let qualified_candidate_trace_provenance = match qualified_candidate_trace_provenance {
            Some(provenance) => {
                self.ensure_qualified_candidate_trace_is_enabled()?;
                Some(provenance.clone_inner())
            }
            None => None,
        };
        let plan = schema.plan();
        validate_assignment_construction_groups(&self.config, plan.schema())?;
        let copy_module = py.import("copy")?;
        let working_solution = copy_module
            .getattr("deepcopy")?
            .call1((solution.bind(py),))?
            .unbind();
        let mut dynamic_solution = import_solution(working_solution.bind(py), plan)?;
        dynamic_solution.solver_config = self.config.clone();
        let score_family = dynamic_solution.schema().score_family.clone();
        let submitted = match qualified_candidate_trace_provenance {
            Some(provenance) => self
                .manager
                .solve_with_qualified_candidate_trace_provenance(dynamic_solution, provenance),
            None => self.manager.solve(dynamic_solution),
        };
        let (job_id, receiver) = submitted.map_err(manager_err_to_py)?;
        self.receivers.lock().insert(job_id, receiver);
        self.originals.lock().insert(job_id, solution.clone_ref(py));
        self.score_families.lock().insert(job_id, score_family);
        Ok(job_id)
    }

    /// Private Python-wrapper preflight. It intentionally validates before
    /// Python schema discovery so an invalid retained diagnostic request never
    /// touches model state, callbacks, or deepcopy/import work.
    #[pyo3(
        name = "_preflight_qualified_candidate_trace_provenance",
        signature = (*, qualified_candidate_trace_provenance = None)
    )]
    fn preflight_qualified_candidate_trace_provenance(
        &self,
        qualified_candidate_trace_provenance: Option<
            PyRef<'_, PyQualifiedCandidateTraceProvenance>,
        >,
    ) -> PyResult<()> {
        if qualified_candidate_trace_provenance.is_some() {
            self.ensure_qualified_candidate_trace_is_enabled()?;
        }
        Ok(())
    }

    fn get_status(&self, py: Python<'_>, job_id: usize) -> PyResult<Py<PyAny>> {
        let status = self.manager.get_status(job_id).map_err(manager_err_to_py)?;
        let score_family = self.score_family(job_id)?;
        status_to_python(py, status, score_family.as_str())
    }

    /// Returns one atomic retained diagnostic view: existing aggregate and
    /// selector/move detail plus the optional bounded candidate trace. Normal
    /// status/event polling remains compact and trace-free.
    fn telemetry_detail(&self, py: Python<'_>, job_id: usize) -> PyResult<Py<PyAny>> {
        let detail = self
            .manager
            .get_telemetry_detail(job_id)
            .map_err(manager_err_to_py)?;
        let score_family = self.score_family(job_id)?;
        telemetry_detail_to_python(py, detail, score_family.as_str())
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
                    let score_family = self.score_family(job_id)?;
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
            let score_family = self.score_family(job_id)?;
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
    fn ensure_qualified_candidate_trace_is_enabled(&self) -> PyResult<()> {
        if self.config.candidate_trace.is_none() {
            return Err(py_err(concat!(
                "qualified_candidate_trace_provenance requires SolverManager configured with ",
                "candidate_trace"
            )));
        }
        Ok(())
    }

    fn score_family(&self, job_id: usize) -> PyResult<String> {
        retained_score_family(&self.score_families, job_id)
    }
}

fn retained_score_family(
    score_families: &Mutex<HashMap<usize, String>>,
    job_id: usize,
) -> PyResult<String> {
    score_families.lock().get(&job_id).cloned().ok_or_else(|| {
        py_err(format!(
            "job {job_id} has no retained score-family provenance"
        ))
    })
}

fn manager_err_to_py(error: SolverManagerError) -> PyErr {
    py_err(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{retained_score_family, HashMap, Mutex};

    #[test]
    fn missing_retained_score_family_is_an_error_not_a_serialization_fallback() {
        pyo3::Python::initialize();
        let score_families = Mutex::new(HashMap::new());

        let error = retained_score_family(&score_families, 42)
            .expect_err("missing provenance must not become hard_medium_soft");

        assert!(error
            .to_string()
            .contains("job 42 has no retained score-family provenance"));
    }
}
