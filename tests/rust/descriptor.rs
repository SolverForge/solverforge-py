use std::sync::Arc;

use pyo3::Python;
use solverforge_bridge::EntityClassId;
use solverforge_core::domain::SolutionDescriptor;
use solverforge_py::schema::build::solution_descriptor;
use solverforge_py::schema::{DynamicSchema, EntitySchema};
use solverforge_py::state::callback_view::PythonCallbackView;
use solverforge_py::state::entity_table::{DynamicEntityRow, DynamicState};
use solverforge_py::state::PyDynamicSolution;

#[test]
fn dynamic_descriptor_extracts_multiple_logical_classes_from_one_row_type() {
    crate::initialize_python();
    Python::attach(|py| {
        let schema = Arc::new(DynamicSchema {
            solution_type: "Plan".to_string(),
            score_family: "hard_soft".to_string(),
            entities: vec![
                EntitySchema {
                    type_name: "Task".to_string(),
                    collection: "tasks".to_string(),
                    variables: Vec::new(),
                },
                EntitySchema {
                    type_name: "Vehicle".to_string(),
                    collection: "vehicles".to_string(),
                    variables: Vec::new(),
                },
            ],
            facts: Vec::new(),
            constraints: py.None(),
            scalar_groups: pyo3::types::PyList::empty(py).unbind().into_any(),
            assignment_scalar_groups: Vec::new(),
            conflict_repairs: pyo3::types::PyList::empty(py).unbind().into_any(),
            shadow_updates: Vec::new(),
        });
        let descriptor: SolutionDescriptor = solution_descriptor(&schema);
        let solution = PyDynamicSolution {
            schema,
            state: DynamicState {
                entities: vec![
                    vec![DynamicEntityRow::default(), DynamicEntityRow::default()],
                    vec![DynamicEntityRow::default()],
                ],
                facts: Vec::new(),
                list_elements: Vec::new(),
                ..DynamicState::default()
            },
            callback_view: PythonCallbackView::default(),
            score: None,
            solver_config: solverforge_config::SolverConfig::default(),
            revision: 0,
        };

        assert_eq!(descriptor.entity_descriptor_count(), 2);
        assert_eq!(descriptor.total_entity_count(&solution), Some(3));
        assert_eq!(
            descriptor
                .find_entity_descriptor_by_logical_id(EntityClassId(0))
                .map(|entity| entity.type_name),
            Some("Task")
        );
        assert_eq!(
            descriptor
                .find_entity_descriptor_by_logical_id(EntityClassId(1))
                .map(|entity| entity.type_name),
            Some("Vehicle")
        );
        assert!(descriptor
            .get_entity(&solution, 0, 1)
            .and_then(|entity| entity.downcast_ref::<DynamicEntityRow>())
            .is_some());
        assert!(descriptor
            .get_entity(&solution, 1, 0)
            .and_then(|entity| entity.downcast_ref::<DynamicEntityRow>())
            .is_some());
    });
}
