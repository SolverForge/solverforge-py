use super::*;
use crate::schema::runtime_plan::CompiledRuntimePlan;
use crate::schema::{DynamicSchema, EntitySchema, VariableSchema};
use crate::state::callback_view::PythonCallbackView;
use crate::state::entity_table::{DynamicEntityRow, DynamicState};
use std::ffi::CString;
use std::sync::Arc;

fn fixture(py: Python<'_>, mixed: bool) -> PyResult<PyDynamicSolution> {
    // Deliberately omit generic filters/weight/impact on precedence plans: any
    // trip through generic dictionary decoding must fail, even in a mixed set.
    let source = CString::new(if mixed {
        "[dict(entity_type='Machine', filters=[], impact='penalty', weight=(0,7)),
          dict(entity_type='Machine', constraint_type='list_precedence_makespan',
               variable_name='operations', score_family='hard_soft',
               precedence_duration_field='durations'),
          dict(entity_type='Machine', filters=[], impact='penalty', weight=(0,1),
               group_key=lambda row: 0, group_filters=[],
               weight_callback=lambda key, count: (0,count*3)),
          dict(entity_type='Machine', constraint_type='list_precedence_makespan',
               variable_name='operations', score_family='hard_soft',
               precedence_duration_field='other_durations'),
          dict(entity_type='Machine', filters=[], impact='penalty', weight=(1,0),
               constraint_type='list_unassigned_element', variable_name='operations')]"
    } else {
        "[dict(entity_type='Machine', constraint_type='list_precedence_makespan',
               variable_name='operations', score_family='hard_soft',
               precedence_duration_field='durations')]"
    })
    .unwrap();
    let constraints = py.eval(&source, None, None)?.unbind();
    let schema = Arc::new(DynamicSchema {
        solution_type: "Schedule".into(),
        score_family: "hard_soft".into(),
        entities: vec![EntitySchema {
            type_name: "Machine".into(),
            collection: "machines".into(),
            variables: vec![VariableSchema {
                name: "operations".into(),
                storage_name: "operations".into(),
                kind: "planning_list_variable".into(),
                element_collection: Some("elements".into()),
                list_metadata: Some(Default::default()),
                ..VariableSchema::default()
            }],
            pin_field: None,
        }],
        facts: Vec::new(),
        constraints,
        scalar_groups: PyList::empty(py).into_any().unbind(),
        assignment_scalar_groups: Vec::new(),
        conflict_repairs: PyList::empty(py).into_any().unbind(),
        candidate_metrics: PyList::empty(py).into_any().unbind(),
        shadow_updates: Vec::new(),
    });
    let runtime_plan = Arc::new(CompiledRuntimePlan::from_schema(schema).map_err(py_err)?);
    let mut row = DynamicEntityRow::with_variable_count(1);
    row.set_list_at(0, vec![0, 1]);
    let mut state = DynamicState {
        entities: vec![vec![row]],
        solution_fields: Arc::new(BTreeMap::from([
            (
                "durations".into(),
                DynamicValue::List(vec![DynamicValue::Int(2), DynamicValue::Int(3)]),
            ),
            (
                "other_durations".into(),
                DynamicValue::List(vec![DynamicValue::Int(5), DynamicValue::Int(7)]),
            ),
        ])),
        ..DynamicState::default()
    };
    state.set_list_elements_at(0, 0, Arc::from([0, 1]));
    Ok(PyDynamicSolution::from_runtime_plan(
        runtime_plan,
        state,
        PythonCallbackView::default(),
        None,
        solverforge_config::SolverConfig::default(),
        0,
    ))
}

#[test]
fn compiled_precedence_never_decodes_generic_rows_or_plan_fields() {
    Python::initialize();
    Python::attach(|py| {
        let mut solution = fixture(py, false).unwrap();
        let mut set = PyDynamicConstraintSet::from_solution(py, &solution).unwrap();
        assert!(matches!(
            set.compiled.plans()[0],
            CompiledConstraintPlan::ListPrecedence(_)
        ));
        let baseline = set.initialize_all(&solution);
        assert_eq!(baseline, DynamicScore::hard_soft(0, -5));
        assert!(set.cached_rows.is_none());
        // Scoring reads no family, so this thread needs no solve scope.
        assert_eq!(set.on_retract_all(&solution, 0, 0), DynamicScore::ZERO);
        // A cached delta must not parse even precedence-specific immutable fields.
        let plan = set.compiled.plans()[0]
            .python_plan()
            .bind(py)
            .cast::<PyDict>()
            .unwrap();
        plan.del_item("score_family").unwrap();
        plan.del_item("variable_name").unwrap();
        solution.state.entities[0][0].set_list_at(0, vec![0]);
        let delta = set.on_insert_all(&solution, 0, 0);
        assert_eq!(baseline + delta, DynamicScore::hard_soft(-1, -3));
        assert!(set.cached_rows.is_none());
        assert_eq!(
            set.list_precedence_cache
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![0]
        );
    });
}

#[test]
fn mixed_compiled_delta_preserves_original_indices_and_scores_each_plan_once() {
    Python::initialize();
    Python::attach(|py| {
        let mut solution = fixture(py, true).unwrap();
        let mut set = PyDynamicConstraintSet::from_solution(py, &solution).unwrap();
        let mut score = set.initialize_all(&solution);
        assert_eq!(score, DynamicScore::hard_soft(0, -27));
        for values in [vec![0], vec![], vec![1, 0], vec![0, 1]] {
            score = score + set.on_retract_all(&solution, 0, 0);
            solution.state.entities[0][0].set_list_at(0, values);
            score = score + set.on_insert_all(&solution, 0, 0);
            assert_eq!(score, set.evaluate_all(&solution));
            assert_eq!(
                set.list_precedence_cache
                    .keys()
                    .copied()
                    .collect::<Vec<_>>(),
                vec![1, 3]
            );
            assert_eq!(set.group_cache.keys().copied().collect::<Vec<_>>(), vec![2]);
            assert_eq!(
                set.list_unassigned_count_cache
                    .keys()
                    .copied()
                    .collect::<Vec<_>>(),
                vec![4]
            );
        }
        set.reset_all();
        assert!(set.list_precedence_cache.is_empty());
        assert!(set.cached_rows.is_none());
        assert_eq!(set.initialize_all(&solution), score);
    });
}
