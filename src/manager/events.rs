use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use solverforge_solver::stats::{
    AppliedMoveTelemetry, MoveTelemetry, SelectorTelemetry, SolverTelemetry,
};
use solverforge_solver::{
    SolverEvent, SolverLifecycleState, SolverStatus, SolverTelemetryDetail, SolverTerminalReason,
};

use super::candidate_trace::candidate_trace_to_python;
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
    let telemetry = telemetry_to_python(py, &status.telemetry)?;
    status_to_python_with_telemetry(py, status, score_family, telemetry)
}

fn status_to_python_with_telemetry(
    py: Python<'_>,
    status: SolverStatus<crate::score::DynamicScore>,
    score_family: &str,
    telemetry: Py<PyAny>,
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
    dict.set_item("telemetry", telemetry)?;
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
    dict.set_item("moves_applied", telemetry.moves_applied)?;
    dict.set_item("moves_score_improving", telemetry.moves_score_improving)?;
    dict.set_item("moves_applied_improving", telemetry.moves_applied_improving)?;
    dict.set_item("moves_not_doable", telemetry.moves_not_doable)?;
    dict.set_item("moves_acceptor_rejected", telemetry.moves_acceptor_rejected)?;
    dict.set_item("moves_forager_ignored", telemetry.moves_forager_ignored)?;
    dict.set_item("moves_hard_improving", telemetry.moves_hard_improving)?;
    dict.set_item("moves_hard_neutral", telemetry.moves_hard_neutral)?;
    dict.set_item("moves_hard_worse", telemetry.moves_hard_worse)?;
    dict.set_item(
        "conflict_repair_provider_generated",
        telemetry.conflict_repair_provider_generated,
    )?;
    dict.set_item(
        "conflict_repair_duplicate_filtered",
        telemetry.conflict_repair_duplicate_filtered,
    )?;
    dict.set_item(
        "conflict_repair_illegal_filtered",
        telemetry.conflict_repair_illegal_filtered,
    )?;
    dict.set_item(
        "conflict_repair_not_doable_filtered",
        telemetry.conflict_repair_not_doable_filtered,
    )?;
    dict.set_item(
        "conflict_repair_hard_improving",
        telemetry.conflict_repair_hard_improving,
    )?;
    dict.set_item("conflict_repair_exposed", telemetry.conflict_repair_exposed)?;
    dict.set_item("score_calculations", telemetry.score_calculations)?;
    dict.set_item(
        "construction_slots_assigned",
        telemetry.construction_slots_assigned,
    )?;
    dict.set_item("construction_slots_kept", telemetry.construction_slots_kept)?;
    dict.set_item(
        "construction_slots_no_doable",
        telemetry.construction_slots_no_doable,
    )?;
    dict.set_item(
        "scalar_assignment_required_remaining",
        telemetry.scalar_assignment_required_remaining,
    )?;
    dict.set_item("generation_ms", duration_millis(telemetry.generation_time))?;
    dict.set_item("evaluation_ms", duration_millis(telemetry.evaluation_time))?;
    if let Some(phase) = &telemetry.phase {
        let phase_dict = PyDict::new(py);
        phase_dict.set_item("phase_index", phase.phase_index)?;
        phase_dict.set_item("phase_type", &phase.phase_type)?;
        phase_dict.set_item("elapsed_ms", duration_millis(phase.elapsed))?;
        phase_dict.set_item("step_count", phase.step_count)?;
        phase_dict.set_item("moves_generated", phase.moves_generated)?;
        phase_dict.set_item("moves_evaluated", phase.moves_evaluated)?;
        phase_dict.set_item("moves_accepted", phase.moves_accepted)?;
        phase_dict.set_item("moves_applied", phase.moves_applied)?;
        phase_dict.set_item("moves_score_improving", phase.moves_score_improving)?;
        phase_dict.set_item("moves_applied_improving", phase.moves_applied_improving)?;
        phase_dict.set_item("score_calculations", phase.score_calculations)?;
        phase_dict.set_item("generation_ms", duration_millis(phase.generation_time))?;
        phase_dict.set_item("evaluation_ms", duration_millis(phase.evaluation_time))?;
        dict.set_item("phase", phase_dict)?;
    } else {
        dict.set_item("phase", py.None())?;
    }
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

/// Returns the full solver-owned work trace on explicit request.  Status and
/// event polling retain their compact aggregate payload so a callback-heavy
/// retained solve does not pay to materialize every selector or applied move.
pub(super) fn telemetry_detail_to_python(
    py: Python<'_>,
    detail: SolverTelemetryDetail<crate::score::DynamicScore>,
    score_family: &str,
) -> PyResult<Py<PyAny>> {
    let status = detail.status;
    let telemetry = detailed_telemetry_to_python(py, &status.telemetry)?;
    let payload = PyDict::new(py);
    payload.set_item(
        "status",
        status_to_python_with_telemetry(py, status, score_family, telemetry)?,
    )?;
    match detail.candidate_trace {
        Some(trace) => {
            payload.set_item("candidate_trace", candidate_trace_to_python(py, &trace)?)?
        }
        None => payload.set_item("candidate_trace", py.None())?,
    }
    Ok(payload.into_any().unbind())
}

/// Adds the pre-existing selector/move/applied-move detail to compact
/// telemetry. This is deliberately trace-free so it remains usable by the
/// atomic detail endpoint without smuggling candidate entries into status or
/// event conversion.
fn detailed_telemetry_to_python(
    py: Python<'_>,
    telemetry: &SolverTelemetry,
) -> PyResult<Py<PyAny>> {
    let payload = telemetry_to_python(py, telemetry)?;
    let dict = payload.bind(py).cast::<PyDict>()?;
    dict.set_item(
        "selector_telemetry",
        selector_telemetry_to_python(py, &telemetry.selector_telemetry)?,
    )?;
    dict.set_item(
        "move_telemetry",
        move_telemetry_to_python(py, &telemetry.move_telemetry)?,
    )?;
    dict.set_item(
        "applied_move_trace",
        applied_move_trace_to_python(py, &telemetry.applied_move_trace)?,
    )?;
    Ok(payload)
}

fn selector_telemetry_to_python(
    py: Python<'_>,
    telemetry: &[SelectorTelemetry],
) -> PyResult<Py<PyAny>> {
    let entries = telemetry
        .iter()
        .map(|entry| {
            let dict = PyDict::new(py);
            dict.set_item("selector_index", entry.selector_index)?;
            dict.set_item("selector_label", &entry.selector_label)?;
            dict.set_item("moves_generated", entry.moves_generated)?;
            dict.set_item("moves_evaluated", entry.moves_evaluated)?;
            dict.set_item("moves_accepted", entry.moves_accepted)?;
            dict.set_item("moves_applied", entry.moves_applied)?;
            dict.set_item("moves_not_doable", entry.moves_not_doable)?;
            dict.set_item("moves_acceptor_rejected", entry.moves_acceptor_rejected)?;
            dict.set_item("moves_forager_ignored", entry.moves_forager_ignored)?;
            dict.set_item("moves_hard_improving", entry.moves_hard_improving)?;
            dict.set_item("moves_hard_neutral", entry.moves_hard_neutral)?;
            dict.set_item("moves_hard_worse", entry.moves_hard_worse)?;
            dict.set_item(
                "conflict_repair_provider_generated",
                entry.conflict_repair_provider_generated,
            )?;
            dict.set_item(
                "conflict_repair_duplicate_filtered",
                entry.conflict_repair_duplicate_filtered,
            )?;
            dict.set_item(
                "conflict_repair_illegal_filtered",
                entry.conflict_repair_illegal_filtered,
            )?;
            dict.set_item(
                "conflict_repair_not_doable_filtered",
                entry.conflict_repair_not_doable_filtered,
            )?;
            dict.set_item(
                "conflict_repair_hard_improving",
                entry.conflict_repair_hard_improving,
            )?;
            dict.set_item("conflict_repair_exposed", entry.conflict_repair_exposed)?;
            dict.set_item("generation_ms", duration_millis(entry.generation_time))?;
            dict.set_item("evaluation_ms", duration_millis(entry.evaluation_time))?;
            Ok(dict.into_any().unbind())
        })
        .collect::<PyResult<Vec<_>>>()?;
    Ok(PyList::new(py, entries)?.into_any().unbind())
}

fn move_telemetry_to_python(py: Python<'_>, telemetry: &[MoveTelemetry]) -> PyResult<Py<PyAny>> {
    let entries = telemetry
        .iter()
        .map(|entry| {
            let dict = PyDict::new(py);
            dict.set_item("move_label", &entry.move_label)?;
            dict.set_item("moves_generated", entry.moves_generated)?;
            dict.set_item("moves_evaluated", entry.moves_evaluated)?;
            dict.set_item("moves_accepted", entry.moves_accepted)?;
            dict.set_item("moves_applied", entry.moves_applied)?;
            dict.set_item("moves_not_doable", entry.moves_not_doable)?;
            dict.set_item("moves_acceptor_rejected", entry.moves_acceptor_rejected)?;
            dict.set_item("moves_forager_ignored", entry.moves_forager_ignored)?;
            dict.set_item("moves_score_improving", entry.moves_score_improving)?;
            dict.set_item("moves_applied_improving", entry.moves_applied_improving)?;
            dict.set_item("moves_score_equal", entry.moves_score_equal)?;
            dict.set_item("moves_score_worse", entry.moves_score_worse)?;
            dict.set_item("moves_rejected_improving", entry.moves_rejected_improving)?;
            dict.set_item("applied_score_improvement", entry.applied_score_improvement)?;
            Ok(dict.into_any().unbind())
        })
        .collect::<PyResult<Vec<_>>>()?;
    Ok(PyList::new(py, entries)?.into_any().unbind())
}

fn applied_move_trace_to_python(
    py: Python<'_>,
    telemetry: &[AppliedMoveTelemetry],
) -> PyResult<Py<PyAny>> {
    let entries = telemetry
        .iter()
        .map(|entry| {
            let dict = PyDict::new(py);
            dict.set_item("step_index", entry.step_index)?;
            dict.set_item("move_label", entry.move_label)?;
            dict.set_item("selected_candidate_index", entry.selected_candidate_index)?;
            dict.set_item("moves_generated_this_step", entry.moves_generated_this_step)?;
            dict.set_item("moves_evaluated_this_step", entry.moves_evaluated_this_step)?;
            dict.set_item("moves_accepted_this_step", entry.moves_accepted_this_step)?;
            dict.set_item(
                "moves_forager_ignored_this_step",
                entry.moves_forager_ignored_this_step,
            )?;
            dict.set_item("score_before", entry.score_before)?;
            dict.set_item("score_after", entry.score_after)?;
            dict.set_item("score_delta", entry.score_delta)?;
            dict.set_item("hard_feasible_before", entry.hard_feasible_before)?;
            dict.set_item("hard_feasible_after", entry.hard_feasible_after)?;
            Ok(dict.into_any().unbind())
        })
        .collect::<PyResult<Vec<_>>>()?;
    Ok(PyList::new(py, entries)?.into_any().unbind())
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use pyo3::prelude::*;
    use pyo3::types::{PyDict, PyList};
    use solverforge_solver::stats::{
        AppliedMoveTelemetry, MoveTelemetry, PhaseTelemetry, SelectorTelemetry, SolverTelemetry,
    };

    use super::detailed_telemetry_to_python;

    #[test]
    fn telemetry_maps_current_phase() {
        Python::initialize();
        let telemetry = SolverTelemetry {
            moves_applied: 5,
            moves_not_doable: 6,
            moves_acceptor_rejected: 7,
            moves_forager_ignored: 8,
            moves_hard_improving: 9,
            moves_hard_neutral: 10,
            moves_hard_worse: 11,
            construction_slots_assigned: 3,
            scalar_assignment_required_remaining: 2,
            selector_telemetry: vec![SelectorTelemetry {
                selector_index: 4,
                selector_label: "grouped nurse".to_string(),
                moves_generated: 12,
                moves_evaluated: 10,
                moves_accepted: 2,
                moves_applied: 1,
                generation_time: Duration::from_millis(4),
                evaluation_time: Duration::from_millis(8),
                ..SelectorTelemetry::default()
            }],
            move_telemetry: vec![MoveTelemetry {
                move_label: "DynamicScalar".to_string(),
                moves_generated: 12,
                moves_evaluated: 10,
                moves_accepted: 2,
                moves_applied: 1,
                applied_score_improvement: 3.5,
                ..MoveTelemetry::default()
            }],
            applied_move_trace: vec![AppliedMoveTelemetry {
                step_index: 3,
                move_label: "DynamicScalar",
                selected_candidate_index: 2,
                moves_generated_this_step: 4,
                moves_evaluated_this_step: 3,
                moves_accepted_this_step: 1,
                score_before: -2.0,
                score_after: 0.0,
                score_delta: 2.0,
                hard_feasible_before: false,
                hard_feasible_after: true,
                ..AppliedMoveTelemetry::default()
            }],
            phase: Some(PhaseTelemetry {
                phase_index: 1,
                phase_type: "Local Search".to_string(),
                elapsed: Duration::from_millis(1_250),
                step_count: 3,
                moves_generated: 12,
                moves_evaluated: 10,
                moves_accepted: 2,
                moves_applied: 1,
                moves_score_improving: 1,
                moves_applied_improving: 1,
                score_calculations: 11,
                generation_time: Duration::from_millis(4),
                evaluation_time: Duration::from_millis(8),
            }),
            ..SolverTelemetry::default()
        };

        Python::attach(|py| {
            let payload = detailed_telemetry_to_python(py, &telemetry).expect("telemetry payload");
            let payload = payload.bind(py).cast::<PyDict>().expect("telemetry dict");
            let phase = payload
                .get_item("phase")
                .expect("phase lookup")
                .expect("phase payload")
                .cast_into::<PyDict>()
                .expect("phase dict");
            assert_eq!(
                phase
                    .get_item("phase_type")
                    .expect("phase type lookup")
                    .expect("phase type")
                    .extract::<String>()
                    .expect("phase type string"),
                "Local Search"
            );
            assert_eq!(
                phase
                    .get_item("moves_applied")
                    .expect("moves applied lookup")
                    .expect("moves applied")
                    .extract::<u64>()
                    .expect("moves applied count"),
                1
            );
            assert_eq!(
                payload
                    .get_item("moves_applied")
                    .expect("moves applied lookup")
                    .expect("moves applied")
                    .extract::<u64>()
                    .expect("moves applied count"),
                5
            );
            assert_eq!(
                payload
                    .get_item("construction_slots_assigned")
                    .expect("construction assigned lookup")
                    .expect("construction assigned")
                    .extract::<u64>()
                    .expect("construction assigned count"),
                3
            );
            assert_eq!(
                payload
                    .get_item("scalar_assignment_required_remaining")
                    .expect("required remaining lookup")
                    .expect("required remaining")
                    .extract::<u64>()
                    .expect("required remaining count"),
                2
            );
            assert_eq!(
                payload
                    .get_item("moves_hard_improving")
                    .expect("hard improving lookup")
                    .expect("hard improving")
                    .extract::<u64>()
                    .expect("hard improving count"),
                9
            );

            let selector = payload
                .get_item("selector_telemetry")
                .expect("selector telemetry lookup")
                .expect("selector telemetry")
                .cast_into::<PyList>()
                .expect("selector telemetry list")
                .get_item(0)
                .expect("selector telemetry entry")
                .cast_into::<PyDict>()
                .expect("selector telemetry dict");
            assert_eq!(
                selector
                    .get_item("selector_label")
                    .expect("selector label lookup")
                    .expect("selector label")
                    .extract::<String>()
                    .expect("selector label string"),
                "grouped nurse"
            );

            let trace = payload
                .get_item("applied_move_trace")
                .expect("move trace lookup")
                .expect("move trace")
                .cast_into::<PyList>()
                .expect("move trace list")
                .get_item(0)
                .expect("move trace entry")
                .cast_into::<PyDict>()
                .expect("move trace dict");
            assert!(trace
                .get_item("hard_feasible_after")
                .expect("hard feasibility lookup")
                .expect("hard feasibility")
                .extract::<bool>()
                .expect("hard feasibility bool"));
        });
    }
}
