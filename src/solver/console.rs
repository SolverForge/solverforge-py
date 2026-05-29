use crate::state::PyDynamicSolution;

pub fn log_dynamic_scale(solution: &PyDynamicSolution) {
    let (list_entity_count, list_element_count) = list_scale(solution);
    if list_entity_count > 0 && list_element_count > 0 {
        solverforge_solver::log_solve_start(list_entity_count, Some(list_element_count), None);
        return;
    }

    solverforge_solver::log_solve_start(
        total_entity_count(solution),
        None,
        Some(scalar_candidate_count(solution)),
    );
}

fn total_entity_count(solution: &PyDynamicSolution) -> usize {
    solution.state.entities.iter().map(Vec::len).sum()
}

fn scalar_candidate_count(solution: &PyDynamicSolution) -> usize {
    let mut total_slots = 0usize;
    let mut total_candidates = 0usize;

    for (entity_index, entity) in solution.schema.entities.iter().enumerate() {
        let Some(rows) = solution.state.entities.get(entity_index) else {
            continue;
        };
        for variable in entity
            .variables
            .iter()
            .filter(|variable| variable.kind == "planning_variable")
        {
            total_slots += rows.len();
            total_candidates += rows
                .iter()
                .map(|row| {
                    row.candidates
                        .get(variable.name.as_str())
                        .map(Vec::len)
                        .unwrap_or(0)
                })
                .sum::<usize>();
        }
    }

    total_candidates
        .saturating_add(total_slots / 2)
        .checked_div(total_slots)
        .unwrap_or(0)
}

fn list_scale(solution: &PyDynamicSolution) -> (usize, usize) {
    let mut entity_count = 0usize;
    let mut element_count = 0usize;

    for (entity_index, entity) in solution.schema.entities.iter().enumerate() {
        let Some(rows) = solution.state.entities.get(entity_index) else {
            continue;
        };
        let Some(list_elements) = solution.state.list_elements.get(entity_index) else {
            continue;
        };
        for variable in entity
            .variables
            .iter()
            .filter(|variable| variable.kind == "planning_list_variable")
        {
            entity_count += rows.len();
            element_count += list_elements
                .get(variable.name.as_str())
                .map(Vec::len)
                .unwrap_or(0);
        }
    }

    (entity_count, element_count)
}
