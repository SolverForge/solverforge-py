pub mod events;
pub mod jobs;

use std::collections::HashMap;

use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use solverforge_config::SolverConfig;
use solverforge_solver::stats::SolverTelemetry;
use solverforge_solver::{
    SolverEvent, SolverLifecycleState, SolverManager as UpstreamSolverManager, SolverManagerError,
    SolverStatus, SolverTerminalReason,
};
use tokio::sync::mpsc;

use crate::config::config_from_python;
use crate::error::py_err;
use crate::schema::{parse_schema, validate::validate_dynamic_schema};
use crate::score::DynamicScorePythonExt;
use crate::state::marshal::{export_solution, import_solution};
use crate::state::PyDynamicSolution;

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
        let mut dynamic_solution = import_solution(solution.bind(py), parsed)?;
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

fn status_to_python(
    py: Python<'_>,
    status: SolverStatus<crate::score::DynamicScore>,
    score_family: &str,
) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("job_id", status.job_id)?;
    dict.set_item(
        "lifecycle_state",
        lifecycle_state_name(status.lifecycle_state),
    )?;
    dict.set_item(
        "terminal_reason",
        status.terminal_reason.map(terminal_reason_name),
    )?;
    dict.set_item("checkpoint_available", status.checkpoint_available)?;
    dict.set_item("event_sequence", status.event_sequence)?;
    dict.set_item("latest_snapshot_revision", status.latest_snapshot_revision)?;
    dict.set_item("telemetry", telemetry_to_python(py, &status.telemetry)?)?;
    if let Some(score) = status.current_score {
        dict.set_item(
            "current_score",
            score.to_python_for_family(py, score_family)?,
        )?;
    }
    if let Some(score) = status.best_score {
        dict.set_item("best_score", score.to_python_for_family(py, score_family)?)?;
    }
    Ok(dict.into_any().unbind())
}

fn event_to_python(
    py: Python<'_>,
    event: SolverEvent<PyDynamicSolution>,
    score_family: &str,
) -> PyResult<Py<PyAny>> {
    let metadata = event.metadata().clone();
    let dict = PyDict::new(py);
    dict.set_item("job_id", metadata.job_id)?;
    dict.set_item("event_sequence", metadata.event_sequence)?;
    dict.set_item(
        "lifecycle_state",
        lifecycle_state_name(metadata.lifecycle_state),
    )?;
    dict.set_item("snapshot_revision", metadata.snapshot_revision)?;
    dict.set_item(
        "terminal_reason",
        metadata.terminal_reason.map(terminal_reason_name),
    )?;
    dict.set_item("telemetry", telemetry_to_python(py, &metadata.telemetry)?)?;
    if let Some(score) = metadata.current_score {
        dict.set_item(
            "current_score",
            score.to_python_for_family(py, score_family)?,
        )?;
    }
    if let Some(score) = metadata.best_score {
        dict.set_item("best_score", score.to_python_for_family(py, score_family)?)?;
    }
    dict.set_item("event_type", event_type_name(&event))?;
    if let SolverEvent::Failed { error, .. } = event {
        dict.set_item("error", error)?;
    }
    Ok(dict.into_any().unbind())
}

fn event_type_name(event: &SolverEvent<PyDynamicSolution>) -> &'static str {
    match event {
        SolverEvent::Progress { .. } => "PROGRESS",
        SolverEvent::BestSolution { .. } => "BEST_SOLUTION",
        SolverEvent::PauseRequested { .. } => "PAUSE_REQUESTED",
        SolverEvent::Paused { .. } => "PAUSED",
        SolverEvent::Resumed { .. } => "RESUMED",
        SolverEvent::Completed { .. } => "COMPLETED",
        SolverEvent::Cancelled { .. } => "CANCELLED",
        SolverEvent::Failed { .. } => "FAILED",
    }
}

fn lifecycle_state_name(state: SolverLifecycleState) -> &'static str {
    match state {
        SolverLifecycleState::Solving => "SOLVING",
        SolverLifecycleState::PauseRequested => "PAUSE_REQUESTED",
        SolverLifecycleState::Paused => "PAUSED",
        SolverLifecycleState::Completed => "COMPLETED",
        SolverLifecycleState::Cancelled => "CANCELLED",
        SolverLifecycleState::Failed => "FAILED",
    }
}

fn terminal_reason_name(reason: SolverTerminalReason) -> &'static str {
    match reason {
        SolverTerminalReason::Completed => "COMPLETED",
        SolverTerminalReason::TerminatedByConfig => "TERMINATED_BY_CONFIG",
        SolverTerminalReason::Cancelled => "CANCELLED",
        SolverTerminalReason::Failed => "FAILED",
    }
}

fn telemetry_to_python(py: Python<'_>, telemetry: &SolverTelemetry) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("elapsed_ms", duration_millis(telemetry.elapsed))?;
    dict.set_item("step_count", telemetry.step_count)?;
    dict.set_item("moves_generated", telemetry.moves_generated)?;
    dict.set_item("moves_evaluated", telemetry.moves_evaluated)?;
    dict.set_item("moves_accepted", telemetry.moves_accepted)?;
    dict.set_item("score_calculations", telemetry.score_calculations)?;
    dict.set_item("generation_ms", duration_millis(telemetry.generation_time))?;
    dict.set_item("evaluation_ms", duration_millis(telemetry.evaluation_time))?;
    dict.set_item(
        "moves_per_second",
        whole_units_per_second(telemetry.moves_evaluated, telemetry.elapsed),
    )?;
    dict.set_item(
        "acceptance_rate",
        if telemetry.moves_evaluated == 0 {
            0.0
        } else {
            telemetry.moves_accepted as f64 / telemetry.moves_evaluated as f64
        },
    )?;
    Ok(dict.into_any().unbind())
}

fn duration_millis(duration: std::time::Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn whole_units_per_second(count: u64, elapsed: std::time::Duration) -> u64 {
    let nanos = elapsed.as_nanos();
    if nanos == 0 {
        0
    } else {
        let per_second = u128::from(count)
            .saturating_mul(1_000_000_000)
            .checked_div(nanos)
            .unwrap_or(0);
        per_second.min(u128::from(u64::MAX)) as u64
    }
}

fn manager_err_to_py(error: SolverManagerError) -> PyErr {
    py_err(error.to_string())
}
