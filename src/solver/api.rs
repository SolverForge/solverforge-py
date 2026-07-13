use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};
use solverforge_bridge::try_run_dynamic_solver_with_config_parts;
use solverforge_solver::SolverRuntime;

use crate::config::config_from_python;
use crate::constraints::PyDynamicConstraintSet;
use crate::error::{panic_to_py_err, py_err};
use crate::runtime::dynamic_assignment_group::validate_assignment_construction_groups;
use crate::schema::compiled::CompiledSchema;
use crate::score::{scoped_dynamic_score_family, score_family_from_name, DynamicScorePythonExt};
use crate::state::marshal::{export_solution, import_solution};
use crate::state::PyDynamicSolution;

#[pyfunction]
pub fn calculate_score(
    py: Python<'_>,
    solution: Py<PyAny>,
    schema: PyRef<'_, CompiledSchema>,
) -> PyResult<Py<PyAny>> {
    let plan = schema.plan();
    let parsed = plan.schema();
    let imported = import_solution(solution.bind(py), Arc::clone(&plan))?;
    let constraints = PyDynamicConstraintSet::from_solution(py, &imported)?;
    let score_family = score_family_from_name(&parsed.score_family)?;
    let score = catch_unwind(AssertUnwindSafe(|| {
        scoped_dynamic_score_family(score_family, || constraints.evaluate_solution(&imported))
    }))
    .map_err(panic_to_py_err)??;
    let py_solution = solution.bind(py);
    export_solution(py_solution, &imported)?;
    py_solution.setattr(
        "score",
        score.to_python_for_family(py, &parsed.score_family)?,
    )?;
    score.to_python_for_family(py, &parsed.score_family)
}

#[pyfunction]
pub fn solve(
    py: Python<'_>,
    solution: Py<PyAny>,
    schema: PyRef<'_, CompiledSchema>,
    config: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let solver_config = config_from_python(config)?;
    if solver_config.candidate_trace.is_some() {
        return Err(py_err(concat!(
            "candidate_trace is available only through SolverManager.telemetry_detail(); ",
            "use SolverManager for an explicit retained diagnostic run"
        )));
    }
    let plan = schema.plan();
    let parsed = plan.schema();
    let py_solution = solution.bind(py);
    let mut imported = import_solution(py_solution, Arc::clone(&plan))?;
    let descriptor = plan.descriptor().clone();
    let constraints = PyDynamicConstraintSet::from_solution(py, &imported)?;
    validate_assignment_construction_groups(&solver_config, parsed)?;
    imported.solver_config = solver_config.clone();
    let score_family = score_family_from_name(&parsed.score_family)?;
    let mut solved = catch_unwind(AssertUnwindSafe(|| {
        scoped_dynamic_score_family(score_family, || {
            try_run_dynamic_solver_with_config_parts(
                imported,
                constraints,
                descriptor,
                dynamic_entity_count,
                SolverRuntime::detached(),
                solver_config,
                30,
                dynamic_log_scale,
                None,
                plan.model().clone(),
            )
            .map_err(|error| py_err(error.to_string()))
        })
    }))
    .map_err(panic_to_py_err)??;
    solved.refresh_all_shadows()?;
    export_solution(py_solution, &solved)?;
    if let Some(score) = solved.score {
        py_solution.setattr(
            "score",
            score.to_python_for_family(py, &parsed.score_family)?,
        )?;
    }
    Ok(solution)
}

fn dynamic_entity_count(solution: &PyDynamicSolution, descriptor_index: usize) -> usize {
    solution.entity_count(descriptor_index)
}

fn dynamic_log_scale(solution: &PyDynamicSolution) {
    super::console::log_dynamic_scale(solution);
}
