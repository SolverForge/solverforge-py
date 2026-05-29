use crate::state::PyDynamicSolution;

pub fn scalar_getter(
    solution: &PyDynamicSolution,
    entity_index: usize,
    variable_index: usize,
) -> Option<usize> {
    let entity = &solution.schema.entities[0];
    let variable = &entity.variables[variable_index];
    solution.state.entities[0][entity_index].scalar(variable.name.as_str())
}

pub fn scalar_setter(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    variable_index: usize,
    value: Option<usize>,
) {
    let entity = &solution.schema.entities[0];
    let variable_name = entity.variables[variable_index].name.clone();
    solution.state.entities[0][entity_index].set_scalar(variable_name.as_str(), value);
}
