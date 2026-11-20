use std::sync::Arc;

use solverforge_bridge::{try_run_dynamic_solver_with_config_parts, DynamicConstraintProvider};
use solverforge_config::SolverConfig;
use solverforge_solver::{
    QualifiedCandidateTraceRunProvenance, RuntimeBuildError, RuntimeBuildResult, ScoreAnalysis,
    Solvable, SolverRuntime,
};

use crate::constraints::PyDynamicConstraintSet;
use crate::error::panic_with_py_err;
use crate::schema::DynamicSchema;
use crate::score::{scoped_dynamic_score_family, score_family_from_name, DynamicScore};
use crate::state::PyDynamicSolution;

type PyConstraintProvider<'a> =
    DynamicConstraintProvider<'a, PyDynamicSolution, PyDynamicConstraintSet>;

impl Solvable for PyDynamicSolution {
    fn solve(
        self,
        runtime: SolverRuntime<Self>,
        qualified_candidate_trace_provenance: Option<QualifiedCandidateTraceRunProvenance>,
    ) -> RuntimeBuildResult<()> {
        let runtime_plan = Arc::clone(self.runtime_plan());
        let config = self.solver_config.clone();
        let descriptor = runtime_plan.descriptor().clone();
        let model = runtime_plan.model().clone();
        let providers = constraint_providers(runtime_plan.schema(), &config)?;
        let score_family = score_family_from_name(&runtime_plan.schema().score_family)
            .unwrap_or_else(panic_with_py_err);
        scoped_dynamic_score_family(score_family, || {
            try_run_dynamic_solver_with_config_parts(
                self,
                &providers,
                descriptor,
                dynamic_entity_count,
                runtime,
                config,
                dynamic_log_scale,
                qualified_candidate_trace_provenance,
                model,
            )
        })
        .map(|_| ())
    }
}

/// The one constraint provider a Python solution declares, named by its
/// provider function so `score_director.constraint_provider` can select it.
///
/// A solution without constraints declares no provider name, so a configured
/// `constraint_provider` fails as an unknown name instead of matching a
/// placeholder.
pub(crate) fn constraint_providers<'a>(
    schema: &'a DynamicSchema,
    config: &SolverConfig,
) -> RuntimeBuildResult<[PyConstraintProvider<'a>; 1]> {
    let name = match (
        schema.constraint_provider.as_deref(),
        config.constraint_provider(),
    ) {
        (Some(name), _) => name,
        (None, None) => "",
        (None, Some(requested)) => {
            return Err(RuntimeBuildError::UnknownConstraintProvider {
                requested: requested.to_string(),
                declared: Vec::new(),
            })
        }
    };
    Ok([DynamicConstraintProvider {
        name,
        constraints: dynamic_constraints,
        analyze: dynamic_analysis,
    }])
}

/// Builds the constraint set for one solution: the working solution and every
/// evaluator replica use this same constructor.
pub(crate) fn dynamic_constraints(solution: &PyDynamicSolution) -> PyDynamicConstraintSet {
    pyo3::Python::attach(|py| {
        PyDynamicConstraintSet::from_solution(py, solution).unwrap_or_else(panic_with_py_err)
    })
}

/// Snapshot analysis bound into the upstream manager. Python constraint sets
/// report no per-constraint metadata, so the analysis carries the exact total
/// score and no constraint breakdown. The Python manager exposes no snapshot
/// analysis endpoint.
fn dynamic_analysis(solution: &PyDynamicSolution) -> ScoreAnalysis<DynamicScore> {
    let score = pyo3::Python::attach(|py| {
        PyDynamicConstraintSet::from_solution(py, solution)
            .and_then(|constraints| constraints.evaluate_solution(solution))
    })
    .unwrap_or_else(panic_with_py_err);
    ScoreAnalysis {
        score,
        constraints: Vec::new(),
    }
}

pub(crate) fn dynamic_entity_count(solution: &PyDynamicSolution, descriptor_index: usize) -> usize {
    solution.entity_count(descriptor_index)
}

pub(crate) fn dynamic_log_scale(solution: &PyDynamicSolution) {
    crate::solver::console::log_dynamic_scale(solution);
}
