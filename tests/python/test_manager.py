from tests.python.test_scalar_solving import Schedule, WorkerPlan
import pytest

from solverforge import SolverManager


def test_manager_returns_completed_handle() -> None:
    manager = SolverManager()
    handle = manager.solve(Schedule())
    status = manager.wait(handle.job_id)
    assert status["lifecycle_state"] == "COMPLETED"
    snapshot = manager.snapshot(handle.job_id)
    assert [shift.nurse for shift in snapshot.shifts] == [0, 0]
    assert any(event["event_type"] == "COMPLETED" for event in manager.events(handle.job_id))
    manager.delete(handle.job_id)
    with pytest.raises(RuntimeError, match="was not found"):
        manager.get_status(handle.job_id)


def test_manager_uses_solver_termination_config() -> None:
    manager = SolverManager(
        {
            "termination": {"best_score_limit": "0"},
            "phases": [
                {"type": "construction_heuristic"},
                {
                    "type": "local_search",
                    "local_search_type": "variable_neighborhood_descent",
                    "neighborhoods": [
                        {
                            "type": "change_move_selector",
                            "entity_class": "Task",
                            "variable_name": "worker",
                        }
                    ],
                    "termination": {"step_count_limit": 100},
                },
            ],
        }
    )
    handle = manager.solve(WorkerPlan())

    status = manager.wait(handle.job_id)

    assert status["lifecycle_state"] == "COMPLETED"
    assert status["terminal_reason"] == "TERMINATED_BY_CONFIG"
    assert status["best_score"] == {"family": "soft", "levels": [0]}
