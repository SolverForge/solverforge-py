use std::sync::Arc;

use solverforge_bridge::run_dynamic_solver_with_config;
use solverforge_solver::{Solvable, SolverRuntime};

use crate::constraints::PyDynamicConstraintSet;
use crate::runtime::dynamic_runtime_model;
use crate::runtime::dynamic_scalar_search::build_dynamic_phases;
use crate::schema::build::solution_descriptor;
use crate::score::{scoped_dynamic_score_family, score_family_from_name};
use crate::state::PyDynamicSolution;

impl Solvable for PyDynamicSolution {
    fn solve(self, runtime: SolverRuntime<Self>) {
        let schema = Arc::clone(&self.schema);
        let config = self.solver_config.clone();
        let descriptor = solution_descriptor(&schema);
        let model = dynamic_runtime_model(&schema, &descriptor).expect(
            "dynamic runtime model built from schema should resolve against its descriptor",
        );
        let phase_schema = Arc::clone(&schema);
        let constraints = pyo3::Python::attach(|py| {
            PyDynamicConstraintSet::new(schema.constraints.clone_ref(py))
        });
        let score_family = score_family_from_name(&schema.score_family)
            .unwrap_or(solverforge_bridge::DynamicScoreFamily::HardMediumSoft);
        let _ = scoped_dynamic_score_family(score_family, || {
            run_dynamic_solver_with_config(
                self,
                constraints,
                descriptor,
                dynamic_entity_count,
                runtime,
                config,
                30,
                dynamic_is_trivial,
                dynamic_log_scale,
                move |config, descriptor| {
                    build_dynamic_phases(config, descriptor, &model, &phase_schema)
                },
            )
        });
    }
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
    crate::solver::console::log_dynamic_scale(solution);
}
