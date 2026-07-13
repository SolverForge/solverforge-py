use std::sync::Arc;

use solverforge_bridge::try_run_dynamic_solver_with_config_parts;
use solverforge_solver::{Solvable, SolverRuntime};

use crate::constraints::PyDynamicConstraintSet;
use crate::error::{panic_with_py_err, py_err};
use crate::score::{scoped_dynamic_score_family, score_family_from_name};
use crate::state::PyDynamicSolution;

impl Solvable for PyDynamicSolution {
    fn solve(
        self,
        runtime: SolverRuntime<Self>,
        qualified_candidate_trace_provenance: Option<
            solverforge_solver::stats::QualifiedCandidateTraceRunProvenance,
        >,
    ) {
        let runtime_plan = Arc::clone(self.runtime_plan());
        let config = self.solver_config.clone();
        let descriptor = runtime_plan.descriptor().clone();
        let model = runtime_plan.model().clone();
        let default_time_limit_secs = runtime_plan.default_time_limit_secs();
        let constraints = pyo3::Python::attach(|py| {
            PyDynamicConstraintSet::from_solution(py, &self).unwrap_or_else(panic_with_py_err)
        });
        let score_family = score_family_from_name(&runtime_plan.schema().score_family)
            .unwrap_or_else(panic_with_py_err);
        let result = scoped_dynamic_score_family(score_family, || {
            try_run_dynamic_solver_with_config_parts(
                self,
                constraints,
                descriptor,
                dynamic_entity_count,
                runtime,
                config,
                default_time_limit_secs,
                dynamic_log_scale,
                qualified_candidate_trace_provenance,
                model,
            )
        });
        result.unwrap_or_else(|error| panic_with_py_err(py_err(error.to_string())));
    }
}

fn dynamic_entity_count(solution: &PyDynamicSolution, descriptor_index: usize) -> usize {
    solution.entity_count(descriptor_index)
}

fn dynamic_log_scale(solution: &PyDynamicSolution) {
    crate::solver::console::log_dynamic_scale(solution);
}
