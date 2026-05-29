use std::collections::BTreeMap;
use std::sync::Arc;

use pyo3::Python;
use solverforge_bridge::{DynamicModelBackend, EntityClassId, VariableId};
use solverforge_py::schema::DynamicSchema;
use solverforge_py::schema::{EntitySchema, VariableSchema};
use solverforge_py::state::entity_table::{DynamicEntityRow, DynamicState};
use solverforge_py::state::PyDynamicSolution;

#[test]
fn dynamic_solution_clone_keeps_independent_state() {
    Python::attach(|py| {
        let schema = Arc::new(DynamicSchema {
            solution_type: "Plan".to_string(),
            score_family: "hard_soft".to_string(),
            entities: Vec::new(),
            facts: Vec::new(),
            constraints: py.None(),
            scalar_groups: pyo3::types::PyList::empty(py).unbind().into_any(),
            conflict_repairs: pyo3::types::PyList::empty(py).unbind().into_any(),
        });
        let solution = PyDynamicSolution {
            schema,
            state: DynamicState::default(),
            score: None,
            solver_config: solverforge_config::SolverConfig::default(),
        };
        let cloned = solution.clone();
        assert_eq!(cloned.state.entities.len(), 0);
    });
}

#[test]
fn dynamic_solution_implements_upstream_backend_contract() {
    Python::attach(|py| {
        let schema = Arc::new(DynamicSchema {
            solution_type: "Plan".to_string(),
            score_family: "hard_soft".to_string(),
            entities: vec![EntitySchema {
                type_name: "Vehicle".to_string(),
                collection: "vehicles".to_string(),
                variables: vec![
                    VariableSchema {
                        name: "depot".to_string(),
                        kind: "planning_variable".to_string(),
                        value_range_provider: None,
                        allows_unassigned: true,
                        element_collection: None,
                    },
                    VariableSchema {
                        name: "visits".to_string(),
                        kind: "planning_list_variable".to_string(),
                        value_range_provider: None,
                        allows_unassigned: false,
                        element_collection: Some("visits".to_string()),
                    },
                ],
            }],
            facts: Vec::new(),
            constraints: py.None(),
            scalar_groups: pyo3::types::PyList::empty(py).unbind().into_any(),
            conflict_repairs: pyo3::types::PyList::empty(py).unbind().into_any(),
        });
        let mut row = DynamicEntityRow::default();
        row.set_scalar("depot", None);
        row.lists.insert("visits".to_string(), vec![0, 2]);
        let mut elements = BTreeMap::new();
        elements.insert("visits".to_string(), vec![0, 2, 9]);
        let mut solution = PyDynamicSolution {
            schema,
            state: DynamicState {
                entities: vec![vec![row]],
                facts: Vec::new(),
                list_elements: vec![elements],
            },
            score: None,
            solver_config: solverforge_config::SolverConfig::default(),
        };

        let vehicle = EntityClassId(0);
        let depot = VariableId(0);
        let visits = VariableId(1);

        assert_eq!(DynamicModelBackend::entity_count(&solution, vehicle), 1);
        assert_eq!(solution.get_scalar(vehicle, 0, depot), None);
        solution.set_scalar(vehicle, 0, depot, Some(4));
        assert_eq!(solution.get_scalar(vehicle, 0, depot), Some(4));
        assert_eq!(solution.list_len(vehicle, 0, visits), 2);
        solution.list_insert(vehicle, 0, visits, 1, 9);
        assert_eq!(solution.list_get(vehicle, 0, visits, 1), Some(9));
        assert_eq!(solution.list_remove(vehicle, 0, visits, 0), Some(0));
        assert_eq!(solution.list_get(vehicle, 0, visits, 0), Some(9));
    });
}
