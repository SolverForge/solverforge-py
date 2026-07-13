use std::collections::BTreeMap;
use std::sync::Arc;

use pyo3::Python;
use solverforge_py::runtime::dynamic_runtime_model;
use solverforge_py::runtime::list_slots::list_slots;
use solverforge_py::runtime::scalar_slots::scalar_slots;
use solverforge_py::schema::build::solution_descriptor;
use solverforge_py::schema::runtime_plan::CompiledRuntimePlan;
use solverforge_py::schema::types::{
    ListCapacityFeasibilitySchema, ListMetadataFieldSourceSchema, ListMetadataSchema,
    ListMetadataSourceSchema, ListRouteMetadataSchema, ListSavingsMetadataSchema,
};
use solverforge_py::schema::{DynamicSchema, EntitySchema, VariableSchema};
use solverforge_py::state::callback_view::PythonCallbackView;
use solverforge_py::state::entity_table::{DynamicEntityRow, DynamicState};
use solverforge_py::state::PyDynamicSolution;
use solverforge_py::value::DynamicValue;
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
                        list_metadata: Some(ListMetadataSchema {
                            route: Some(ListRouteMetadataSchema {
                                depot: ListMetadataSourceSchema::Row("depot".to_string()),
                                distance: ListMetadataSourceSchema::Row("matrix".to_string()),
                                feasible: ListMetadataSourceSchema::Capacity(
                                    ListCapacityFeasibilitySchema {
                                        capacity: ListMetadataFieldSourceSchema::Row(
                                            "capacity".to_string(),
                                        ),
                                        demand: ListMetadataFieldSourceSchema::Row(
                                            "demands".to_string(),
                                        ),
                                    },
                                ),
                            }),
                            savings: Some(ListSavingsMetadataSchema {
                                depot: ListMetadataSourceSchema::Row("depot".to_string()),
                                metric_class: ListMetadataSourceSchema::Row(
                                    "metric_class".to_string(),
                                ),
                                distance: ListMetadataSourceSchema::Row("matrix".to_string()),
                                feasible: ListMetadataSourceSchema::Capacity(
                                    ListCapacityFeasibilitySchema {
                                        capacity: ListMetadataFieldSourceSchema::Row(
                                            "capacity".to_string(),
                                        ),
                                        demand: ListMetadataFieldSourceSchema::Row(
                                            "demands".to_string(),
                                        ),
                                    },
                                ),
                            }),
                            cross_position_distance: Some(ListMetadataSourceSchema::Row(
                                "matrix".to_string(),
                            )),
                            intra_position_distance: Some(ListMetadataSourceSchema::Row(
                                "matrix".to_string(),
                            )),
                        }),
                        ..Default::default()
                    },
                ],
            }],
            facts: Vec::new(),
            constraints: py.None(),
            scalar_groups: pyo3::types::PyList::empty(py).unbind().into_any(),
            assignment_scalar_groups: Vec::new(),
            conflict_repairs: pyo3::types::PyList::empty(py).unbind().into_any(),
            candidate_metrics: pyo3::types::PyList::empty(py).unbind().into_any(),
            shadow_updates: Vec::new(),
        });
        let mut row = DynamicEntityRow::default();
        row.set_scalar_at(0, None);
        row.set_list_at(1, vec![1, 3]);
        row.set_field("depot".to_string(), DynamicValue::Int(0));
        row.set_field("metric_class".to_string(), DynamicValue::Int(7));
        row.set_field(
            "matrix".to_string(),
            DynamicValue::List(vec![
                DynamicValue::List(vec![
                    DynamicValue::Int(0),
                    DynamicValue::Int(11),
                    DynamicValue::Int(12),
                    DynamicValue::Int(13),
                ]),
                DynamicValue::List(vec![
                    DynamicValue::Int(21),
                    DynamicValue::Int(0),
                    DynamicValue::Int(23),
                    DynamicValue::Int(24),
                ]),
                DynamicValue::List(vec![
                    DynamicValue::Int(31),
                    DynamicValue::Int(32),
                    DynamicValue::Int(0),
                    DynamicValue::Int(34),
                ]),
                DynamicValue::List(vec![
                    DynamicValue::Int(41),
                    DynamicValue::Int(42),
                    DynamicValue::Int(43),
                    DynamicValue::Int(0),
                ]),
            ]),
        );
        row.set_field("capacity".to_string(), DynamicValue::Int(3));
        row.set_field(
            "demands".to_string(),
            DynamicValue::List(vec![
                DynamicValue::Int(0),
                DynamicValue::Int(2),
                DynamicValue::Int(3),
                DynamicValue::Int(1),
            ]),
        );
        let runtime_plan = Arc::new(
            CompiledRuntimePlan::from_schema(Arc::clone(&schema))
                .expect("test dynamic schema should compile into one runtime plan"),
        );
        let mut solution = PyDynamicSolution::from_runtime_plan(
            runtime_plan,
            DynamicState {
                entities: vec![vec![row]],
                facts: Vec::new(),
                list_elements: vec![vec![None, Some(Arc::<[usize]>::from(vec![1, 3]))]],
                ..DynamicState::default()
            },
            PythonCallbackView::default(),
            None,
            solverforge_config::SolverConfig::default(),
            0,
        );

        let scalar = scalar_slots(&schema).remove(0);
        assert_eq!(scalar.entity_type_name, "Vehicle");
        assert_eq!(scalar.variable_name, "worker");
        assert_eq!(scalar.current_value(&solution, 0), None);
        scalar.set_value(&mut solution, 0, Some(7));
        assert_eq!(scalar.current_value(&solution, 0), Some(7));

        let list = list_slots(&schema)
            .expect("dynamic list slots should compile from schema")
            .remove(0);
        assert_eq!(list.entity_type_name, "Vehicle");
        assert_eq!(list.variable_name, "visits");
        assert_eq!(list.entity_count(&solution), 1);
        assert_eq!(list.element_count(&solution), 2);
        assert_eq!(list.assigned_elements(&solution), vec![1, 3]);
        list.list_insert(&mut solution, 0, 1, 2);
        assert_eq!(list.list_get(&solution, 0, 1), Some(2));
        assert_eq!(list.list_remove(&mut solution, 0, 0), Some(1));
        assert_eq!(list.assigned_elements(&solution), vec![2, 3]);
        let metadata = list
            .metadata()
            .expect("every Python list slot has immutable metadata binding");
        let capabilities = metadata.capabilities();
        assert!(capabilities.route);
        assert!(capabilities.savings);
        assert!(capabilities.cross_position_distance);
        assert!(capabilities.intra_position_distance);
        assert_eq!(metadata.route_depot(&solution, 0), Some(0));
        assert_eq!(metadata.route_distance(&solution, 0, 1, 3), Some(24));
        assert_eq!(metadata.route_feasible(&solution, 0, &[1, 3]), Some(true));
        assert_eq!(metadata.route_feasible(&solution, 0, &[1, 2]), Some(false));
        assert_eq!(metadata.savings_metric_class(&solution, 0), Some(7));
        assert_eq!(metadata.savings_distance(&solution, 0, 3, 1), Some(42));
        assert_eq!(
            metadata.cross_position_distance(&solution, 0, 0, 0, 1),
            Some(34.0),
            "the matrix is indexed by the list values 2 and 3, never positions 0 and 1"
        );
        assert_eq!(
            metadata.intra_position_distance(&solution, 0, 0, 1),
            Some(34.0),
            "the intra-route metric also uses actual list values"
        );

        let mut no_row_source = solution.clone();
        no_row_source.state.entities[0][0].fields = Arc::new(BTreeMap::new());
        no_row_source.state.solution_fields = Arc::new(BTreeMap::from([(
            "depot".to_string(),
            DynamicValue::Int(99),
        )]));
        assert_eq!(
            metadata.route_depot(&no_row_source, 0),
            None,
            "a declared row source must never borrow a same-named solution field"
        );

        let descriptor = solution_descriptor(&schema);
        let model = dynamic_runtime_model(Arc::clone(&schema), &descriptor)
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
