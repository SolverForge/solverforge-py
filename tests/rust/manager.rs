use solverforge_py::manager::NativeSolverManager;

#[test]
fn manager_bridge_type_exists() {
    assert!(std::any::type_name::<NativeSolverManager>().contains("NativeSolverManager"));
}
