use std::collections::BTreeMap;
use std::sync::Arc;

use pyo3::Python;
use solverforge_py::runtime::distance::PyDistanceMeter;
use solverforge_py::runtime::dynamic_runtime_model;
use solverforge_py::runtime::list_slots::list_slots;
use solverforge_py::runtime::scalar_slots::scalar_slots;
use solverforge_py::schema::build::solution_descriptor;
use solverforge_py::schema::{DynamicSchema, EntitySchema, VariableSchema};
use solverforge_py::state::callback_view::PythonCallbackView;
use solverforge_py::state::entity_table::{DynamicEntityRow, DynamicState};
use solverforge_py::state::PyDynamicSolution;
use solverforge_solver::CrossEntityDistanceMeter;

#[test]
fn default_distance_meter_is_deterministic() {
    let meter = PyDistanceMeter;
    assert_eq!(meter.distance(&(), 0, 1, 2, 3), 4.0);
}

#[test]
fn dynamic_runtime_slots_are_built_from_schema_and_drive_state() {
    crate::initialize_python();
    Python::attach(|py| {
        let schema = Arc::new(DynamicSchema {
            solution_type: "Plan".to_string(),
            score_family: "hard_soft".to_string(),
            entities: vec![EntitySchema {
                type_name: "Vehicle".to_string(),
                collection: "vehicles".to_string(),
                variables: vec![
                    VariableSchema {
                        name: "worker".to_string(),
                        storage_name: "__solverforge_worker".to_string(),
                        kind: "planning_variable".to_string(),
                        allows_unassigned: true,
                        ..Default::default()
                    },
                    VariableSchema {
                        name: "visits".to_string(),
                        storage_name: "__solverforge_visits".to_string(),
                        kind: "planning_list_variable".to_string(),
                        allows_unassigned: false,
                        element_collection: Some("visits".to_string()),
                        ..Default::default()
                    },
                ],
            }],
            facts: Vec::new(),
            constraints: py.None(),
            scalar_groups: pyo3::types::PyList::empty(py).unbind().into_any(),
            assignment_scalar_groups: Vec::new(),
            conflict_repairs: pyo3::types::PyList::empty(py).unbind().into_any(),
            shadow_updates: Vec::new(),
        });
        let mut row = DynamicEntityRow::default();
        row.set_scalar("worker", None);
        row.lists.insert("visits".to_string(), vec![1, 3]);
        let mut elements = BTreeMap::new();
        elements.insert("visits".to_string(), vec![1, 3]);
        let mut solution = PyDynamicSolution {
            schema: Arc::clone(&schema),
            state: DynamicState {
                entities: vec![vec![row]],
                facts: Vec::new(),
                list_elements: vec![elements],
                ..DynamicState::default()
            },
            callback_view: PythonCallbackView::default(),
            score: None,
            solver_config: solverforge_config::SolverConfig::default(),
            revision: 0,
        };

        let scalar = scalar_slots(&schema).remove(0);
        assert_eq!(scalar.entity_type_name, "Vehicle");
        assert_eq!(scalar.variable_name, "worker");
        assert_eq!(scalar.current_value(&solution, 0), None);
        scalar.set_value(&mut solution, 0, Some(7));
        assert_eq!(scalar.current_value(&solution, 0), Some(7));

        let list = list_slots(&schema).remove(0);
        assert_eq!(list.entity_type_name, "Vehicle");
        assert_eq!(list.variable_name, "visits");
        assert_eq!(list.entity_count(&solution), 1);
        assert_eq!(list.element_count(&solution), 2);
        assert_eq!(list.assigned_elements(&solution), vec![1, 3]);
        list.list_insert(&mut solution, 0, 1, 2);
        assert_eq!(list.list_get(&solution, 0, 1), Some(2));
        assert_eq!(list.list_remove(&mut solution, 0, 0), Some(1));
        assert_eq!(list.assigned_elements(&solution), vec![2, 3]);

        let descriptor = solution_descriptor(&schema);
        let model = dynamic_runtime_model(&schema, &descriptor)
            .expect("runtime model should resolve against schema descriptor");
        assert!(model.has_scalar_variables());
        assert!(model.has_list_variables());
        assert_eq!(model.dynamic_scalar_variables().count(), 1);
        assert_eq!(model.dynamic_list_variables().count(), 1);
        assert!(model
            .dynamic_scalar_variables()
            .all(|slot| slot.is_descriptor_resolved()));
        assert!(model
            .dynamic_list_variables()
            .all(|slot| slot.is_descriptor_resolved()));
    });
}
