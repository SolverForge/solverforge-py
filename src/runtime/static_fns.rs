use solverforge_core::domain::SolutionDescriptor;
use solverforge_core::PlanningSolution;

use crate::schema::build::solution_descriptor;
use crate::state::PyDynamicSolution;

pub fn descriptor_fn() -> SolutionDescriptor {
    let schema = super::thread_local::active_schema()
        .expect("solverforge-py descriptor requested without active schema");
    solution_descriptor(&schema)
}

pub fn entity_count_by_descriptor(solution: &PyDynamicSolution, descriptor_index: usize) -> usize {
    solution.entity_count(descriptor_index)
}

pub fn is_trivial(solution: &PyDynamicSolution) -> bool {
    solution.is_initialized()
}

pub fn log_scale(_solution: &PyDynamicSolution) {}
