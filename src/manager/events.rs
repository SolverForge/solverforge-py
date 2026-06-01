use pyo3::prelude::*;
use pyo3::types::PyDict;
use solverforge_solver::stats::SolverTelemetry;
use solverforge_solver::{SolverEvent, SolverLifecycleState, SolverStatus, SolverTerminalReason};

use crate::score::DynamicScorePythonExt;
use crate::state::PyDynamicSolution;

#[derive(Debug, Clone)]
pub struct PySolverEvent {
    pub job_id: usize,
    pub lifecycle_state: String,
}

pub(super) fn status_to_python(
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

pub(super) fn event_to_python(
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
