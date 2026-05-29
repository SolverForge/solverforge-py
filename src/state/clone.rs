use super::PyDynamicSolution;

pub fn clone_solution(solution: &PyDynamicSolution) -> PyDynamicSolution {
    solution.clone()
}
