use crate::state::PyDynamicSolution;

pub fn entity_count(solution: &PyDynamicSolution, descriptor_index: usize) -> usize {
    solution.entity_count(descriptor_index)
}
