use std::collections::BTreeMap;
use std::sync::Arc;

use pyo3::types::{PyAnyMethods, PyDict, PyDictMethods, PyList};
use pyo3::Python;
use solverforge_bridge::{DynamicModelBackend, EntityClassId, VariableId};
use solverforge_py::schema::runtime_plan::CompiledRuntimePlan;
use solverforge_py::schema::types::ListMetadataSchema;
use solverforge_py::schema::DynamicSchema;
use solverforge_py::schema::{EntitySchema, VariableSchema};
use solverforge_py::state::callback_view::PythonCallbackView;
use solverforge_py::state::entity_table::{DynamicEntityRow, DynamicState};
use solverforge_py::state::PyDynamicSolution;
use solverforge_py::value::DynamicValue;

fn runtime_plan(schema: &Arc<DynamicSchema>) -> Arc<CompiledRuntimePlan> {
    Arc::new(
        CompiledRuntimePlan::from_schema(Arc::clone(schema))
            .expect("test dynamic schema should compile into one runtime plan"),
    )
}

#[test]
fn dynamic_solution_clone_keeps_independent_state() {
    crate::initialize_python();
    Python::attach(|py| {
        let schema = Arc::new(DynamicSchema {
            solution_type: "Plan".to_string(),
            score_family: "hard_soft".to_string(),
            entities: Vec::new(),
            facts: Vec::new(),
            constraints: py.None(),
            scalar_groups: pyo3::types::PyList::empty(py).unbind().into_any(),
            assignment_scalar_groups: Vec::new(),
            conflict_repairs: pyo3::types::PyList::empty(py).unbind().into_any(),
            candidate_metrics: pyo3::types::PyList::empty(py).unbind().into_any(),
            shadow_updates: Vec::new(),
        });
        let solution = PyDynamicSolution::from_runtime_plan(
            runtime_plan(&schema),
            DynamicState::default(),
            PythonCallbackView::default(),
            None,
            solverforge_config::SolverConfig::default(),
            0,
        );
        let cloned = solution.clone();
        assert_eq!(cloned.state.entities.len(), 0);
    });
}

#[test]
fn dynamic_solution_clone_does_not_share_python_callback_view() {
    crate::initialize_python();
    Python::attach(|py| {
        let namespace = py
            .import("types")
            .unwrap()
            .getattr("SimpleNamespace")
            .unwrap();
        let row_kwargs = PyDict::new(py);
        row_kwargs
            .set_item("__solverforge_visits", Vec::<usize>::new())
            .unwrap();
        let row_object = namespace.call((), Some(&row_kwargs)).unwrap();
        let solution_kwargs = PyDict::new(py);
        solution_kwargs
            .set_item("vehicles", PyList::new(py, [row_object.clone()]).unwrap())
            .unwrap();
        let capacities = PyDict::new(py);
        capacities.set_item(1, 7).unwrap();
        solution_kwargs.set_item("capacities", &capacities).unwrap();
        let python_solution = namespace.call((), Some(&solution_kwargs)).unwrap();
        let schema = Arc::new(DynamicSchema {
            solution_type: "Plan".to_string(),
            score_family: "hard_soft".to_string(),
            entities: vec![EntitySchema {
                type_name: "Vehicle".to_string(),
                collection: "vehicles".to_string(),
                variables: vec![VariableSchema {
                    name: "visits".to_string(),
                    storage_name: "__solverforge_visits".to_string(),
                    kind: "planning_list_variable".to_string(),
                    allows_unassigned: false,
                    element_collection: Some("visits".to_string()),
                    list_metadata: Some(ListMetadataSchema::default()),
                    ..Default::default()
                }],
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
        row.set_list_at(0, Vec::new());
        let mut root_fields = BTreeMap::new();
        root_fields.insert("capacities".to_string(), capacities.unbind().into_any());
        let solution = PyDynamicSolution::from_runtime_plan(
            runtime_plan(&schema),
            DynamicState {
                entities: vec![vec![row]],
                ..DynamicState::default()
            },
            PythonCallbackView::from_import(
                python_solution.unbind(),
                vec![vec![row_object.clone().unbind()]],
                Vec::new(),
                root_fields,
            ),
            None,
            solverforge_config::SolverConfig::default(),
            0,
        );

        let mut cloned = solution.clone();
        cloned.state.entities[0][0].set_list_at(0, vec![1, 2]);
        let clone_solution = cloned.to_python_callback_view(py).unwrap();
        assert_eq!(
            clone_solution
                .bind(py)
                .getattr("capacities")
                .unwrap()
                .get_item(1)
                .unwrap()
                .extract::<usize>()
                .unwrap(),
            7
        );
        assert_eq!(
            clone_solution
                .bind(py)
                .getattr("vehicles")
                .unwrap()
                .get_item(0)
                .unwrap()
                .getattr("visits")
                .unwrap()
                .extract::<Vec<usize>>()
                .unwrap(),
            vec![1, 2]
        );
        let clone_row = cloned.entity_callback_view(py, 0, 0).unwrap();

        assert_eq!(
            clone_row
                .bind(py)
                .getattr("visits")
                .unwrap()
                .extract::<Vec<usize>>()
                .unwrap(),
            vec![1, 2]
        );
        assert_eq!(
            row_object
                .getattr("__solverforge_visits")
                .unwrap()
                .extract::<Vec<usize>>()
                .unwrap(),
            Vec::<usize>::new()
        );
        assert!(row_object.getattr("_solverforge_entity_index").is_err());
    });
}

#[test]
fn dynamic_solution_implements_upstream_backend_contract() {
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
                        name: "depot".to_string(),
                        storage_name: "__solverforge_depot".to_string(),
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
                        list_metadata: Some(ListMetadataSchema::default()),
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
        row.set_list_at(1, vec![0, 2]);
        let mut solution = PyDynamicSolution::from_runtime_plan(
            runtime_plan(&schema),
            DynamicState {
                entities: vec![vec![row]],
                facts: Vec::new(),
                list_elements: vec![vec![None, Some(Arc::<[usize]>::from(vec![0, 2, 9]))]],
                ..DynamicState::default()
            },
            PythonCallbackView::default(),
            None,
            solverforge_config::SolverConfig::default(),
            0,
        );

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

#[test]
fn provider_candidates_can_be_shared_across_rows() {
    let shared = Arc::<[usize]>::from(vec![1, 2, 3]);
    let mut first = DynamicEntityRow::with_variable_count(1);
    let mut second = DynamicEntityRow::with_variable_count(1);

    first.set_candidate_arc_at(0, Arc::clone(&shared));
    second.set_candidate_arc_at(0, Arc::clone(&shared));

    assert_eq!(first.candidates_at(0), Some(&[1, 2, 3][..]));
    assert_eq!(second.candidates_at(0), Some(&[1, 2, 3][..]));
    assert!(Arc::ptr_eq(
        first.candidate_values[0].as_ref().unwrap(),
        second.candidate_values[0].as_ref().unwrap()
    ));
}

#[test]
fn dynamic_solution_clone_copy_on_writes_static_fields() {
    crate::initialize_python();
    Python::attach(|py| {
        let schema = Arc::new(DynamicSchema {
            solution_type: "Plan".to_string(),
            score_family: "hard_soft".to_string(),
            entities: Vec::new(),
            facts: Vec::new(),
            constraints: py.None(),
            scalar_groups: PyList::empty(py).unbind().into_any(),
            assignment_scalar_groups: Vec::new(),
            conflict_repairs: PyList::empty(py).unbind().into_any(),
            candidate_metrics: PyList::empty(py).unbind().into_any(),
            shadow_updates: Vec::new(),
        });
        let mut row = DynamicEntityRow::default();
        row.set_field(
            "metadata".to_string(),
            DynamicValue::List(vec![DynamicValue::Int(7)]),
        );
        let solution = PyDynamicSolution::from_runtime_plan(
            runtime_plan(&schema),
            DynamicState {
                entities: vec![vec![row]],
                ..DynamicState::default()
            },
            PythonCallbackView::default(),
            None,
            solverforge_config::SolverConfig::default(),
            0,
        );

        let mut cloned = solution.clone();
        assert!(Arc::ptr_eq(
            &solution.state.entities[0][0].fields,
            &cloned.state.entities[0][0].fields
        ));
        cloned.state.entities[0][0].set_field("shadow".to_string(), DynamicValue::Int(3));

        assert!(!Arc::ptr_eq(
            &solution.state.entities[0][0].fields,
            &cloned.state.entities[0][0].fields
        ));
        assert_eq!(solution.state.entities[0][0].fields.get("shadow"), None);
        assert_eq!(
            cloned.state.entities[0][0].fields.get("metadata"),
            Some(&DynamicValue::List(vec![DynamicValue::Int(7)]))
        );
    });
}
