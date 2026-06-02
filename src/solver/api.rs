use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use solverforge_bridge::run_dynamic_solver_with_config;
use solverforge_solver::SolverRuntime;

use crate::config::config_from_python;
use crate::constraints::PyDynamicConstraintSet;
use crate::error::py_err;
use crate::runtime::dynamic_runtime_model;
use crate::runtime::dynamic_scalar_search::build_dynamic_phases;
use crate::schema::build::solution_descriptor;
use crate::schema::{parse_schema, validate::validate_dynamic_schema};
use crate::score::{scoped_dynamic_score_family, score_family_from_name, DynamicScorePythonExt};
use crate::state::marshal::{export_solution, import_solution};
use crate::state::PyDynamicSolution;

#[pyfunction]
pub fn calculate_score(
    py: Python<'_>,
    solution: Py<PyAny>,
    schema: &Bound<'_, PyDict>,
) -> PyResult<Py<PyAny>> {
    let parsed = parse_schema(schema)?;
    validate_dynamic_schema(&parsed)?;
    let parsed = Arc::new(parsed);
    let imported = import_solution(solution.bind(py), Arc::clone(&parsed))?;
    let constraints = PyDynamicConstraintSet::new(parsed.constraints.clone_ref(py));
    let score_family = score_family_from_name(&parsed.score_family)?;
    let score = catch_unwind(AssertUnwindSafe(|| {
        scoped_dynamic_score_family(score_family, || constraints.evaluate_solution(&imported))
    }))
    .map_err(panic_to_py_err)??;
    let py_solution = solution.bind(py);
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
    schema: &Bound<'_, PyDict>,
    config: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let parsed = Arc::new(parse_schema(schema)?);
    validate_dynamic_schema(&parsed)?;
    let py_solution = solution.bind(py);
    let mut imported = import_solution(py_solution, Arc::clone(&parsed))?;
    let descriptor = solution_descriptor(&parsed);
    let model = dynamic_runtime_model(&parsed, &descriptor)
        .map_err(|err| py_err(format!("failed to resolve dynamic runtime model: {err}")))?;
    let constraints = PyDynamicConstraintSet::new(parsed.constraints.clone_ref(py));
    let solver_config = config_from_python(config)?;
    imported.solver_config = solver_config.clone();
    let score_family = score_family_from_name(&parsed.score_family)?;
    let solved = catch_unwind(AssertUnwindSafe(|| {
        scoped_dynamic_score_family(score_family, || {
            run_dynamic_solver_with_config(
                imported,
                constraints,
                descriptor,
                dynamic_entity_count,
                SolverRuntime::detached(),
                solver_config,
                30,
                dynamic_is_trivial,
                dynamic_log_scale,
                move |config, descriptor| build_dynamic_phases(config, descriptor, &model),
            )
        })
    }))
    .map_err(panic_to_py_err)?;
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

fn dynamic_is_trivial(solution: &PyDynamicSolution) -> bool {
    solution
        .schema
        .entities
        .iter()
        .all(|entity| entity.variables.is_empty())
}

fn dynamic_log_scale(solution: &PyDynamicSolution) {
    super::console::log_dynamic_scale(solution);
}

fn panic_to_py_err(payload: Box<dyn std::any::Any + Send>) -> PyErr {
    let message = if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "solver panicked with a non-string payload".to_string()
    };
    py_err(message)
}
