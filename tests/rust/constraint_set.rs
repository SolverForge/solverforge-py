use pyo3::Python;
use solverforge_py::constraints::PyDynamicConstraintSet;
use solverforge_py::schema::DynamicSchema;
use solverforge_py::score::DynamicScore;
use solverforge_py::state::callback_view::PythonCallbackView;
use solverforge_py::state::entity_table::DynamicState;
use solverforge_py::state::PyDynamicSolution;
use solverforge_scoring::ConstraintSet;
use std::sync::Arc;

#[test]
fn dynamic_constraint_set_reports_constraint_count() {
    crate::initialize_python();
    Python::attach(|py| {
        let constraints = pyo3::types::PyList::empty(py).unbind().into_any();
        let set = PyDynamicConstraintSet::new(constraints);
        assert_eq!(set.constraint_count(), 0);
        let solution = PyDynamicSolution {
            schema: Arc::new(DynamicSchema {
                solution_type: "Plan".to_string(),
                score_family: "hard_soft".to_string(),
                entities: Vec::new(),
                facts: Vec::new(),
                constraints: py.None(),
                scalar_groups: pyo3::types::PyList::empty(py).unbind().into_any(),
                assignment_scalar_groups: Vec::new(),
                conflict_repairs: pyo3::types::PyList::empty(py).unbind().into_any(),
                shadow_updates: Vec::new(),
            }),
            state: DynamicState::default(),
            callback_view: PythonCallbackView::default(),
            score: Some(DynamicScore::ZERO),
            solver_config: solverforge_config::SolverConfig::default(),
            revision: 0,
        };
        assert_eq!(set.evaluate_all(&solution), DynamicScore::ZERO);
    });
}
