use crate::state::PyDynamicSolution;

pub fn scalar_getter(
    solution: &PyDynamicSolution,
    entity_index: usize,
    variable_index: usize,
) -> Option<usize> {
    solution.state.entities[0][entity_index].scalar_at(variable_index)
}

pub fn scalar_setter(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    variable_index: usize,
    value: Option<usize>,
) {
    solution.set_scalar_value(0, entity_index, variable_index, value);
}
