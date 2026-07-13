from collections.abc import Callable

from tests.python.test_scalar_solving import AssignmentSchedule, Schedule, WorkerPlan
from tests.python.test_tracebacks import (
    BROKEN_GROUP_CONFIG,
    BROKEN_REPAIR_CONFIG,
    BrokenGroupProviderPlan,
    BrokenPlan,
    BrokenRepairProviderPlan,
)
import pytest

from solverforge import (
    HardMediumSoftScore,
    HardSoftDecimalScore,
    Solver,
    SolverManager,
    planning_entity,
    planning_solution,
    planning_variable,
)


@planning_entity
class ScoreFamilyTask:
    worker = planning_variable(value_range_provider="score_family_workers")

    def __init__(self) -> None:
        self.worker: int | None = None


@planning_solution(score=HardSoftDecimalScore)
class DecimalScoreFamilyPlan:
    score_family_tasks: list[ScoreFamilyTask]

    def __init__(self) -> None:
        self.score_family_tasks = [ScoreFamilyTask()]
        self.score_family_workers = [0]
        self.score = None


@planning_solution(score=HardMediumSoftScore)
class MediumScoreFamilyPlan:
    score_family_tasks: list[ScoreFamilyTask]

    def __init__(self) -> None:
        self.score_family_tasks = [ScoreFamilyTask()]
        self.score_family_workers = [0]
        self.score = None


def terminal_events(events: list[dict[str, object]]) -> list[dict[str, object]]:
    return [
        event
        for event in events
        if event["event_type"] in {"COMPLETED", "CANCELLED", "FAILED"}
    ]


def test_manager_returns_completed_handle() -> None:
    manager = SolverManager()
    handle = manager.solve(Schedule())
    status = manager.wait(handle.job_id)
    assert status["lifecycle_state"] == "COMPLETED"
    telemetry = status["telemetry"]
    assert isinstance(telemetry, dict)
    assert "selector_telemetry" not in telemetry
    detail = manager.telemetry_detail(handle.job_id)
    assert isinstance(detail["status"], dict)
    detail_status = detail["status"]
    detail_telemetry = detail_status["telemetry"]
    assert isinstance(detail_telemetry, dict)
    assert isinstance(detail_telemetry["selector_telemetry"], list)
    assert isinstance(detail_telemetry["move_telemetry"], list)
    assert isinstance(detail_telemetry["applied_move_trace"], list)
    assert detail_status["job_id"] == status["job_id"]
    assert detail_status["lifecycle_state"] == status["lifecycle_state"]
    assert detail_status["terminal_reason"] == status["terminal_reason"]
    assert detail_status["event_sequence"] == status["event_sequence"]
    assert (
        detail_status["latest_snapshot_revision"] == status["latest_snapshot_revision"]
    )
    assert detail["candidate_trace"] is None
    snapshot = manager.snapshot(handle.job_id)
    assert [shift.nurse for shift in snapshot.shifts] == [0, 0]
    events = manager.events(handle.job_id)
    assert [event["event_type"] for event in terminal_events(events)] == ["COMPLETED"]
    manager.delete(handle.job_id)
    with pytest.raises(RuntimeError, match="was not found"):
        manager.get_status(handle.job_id)


def test_manager_candidate_trace_is_atomic_and_excluded_from_control_plane() -> None:
    manager = SolverManager({"candidate_trace": {"max_entries": 128}})
    handle = manager.solve(Schedule())
    status = manager.wait(handle.job_id)
    events = manager.events(handle.job_id)

    telemetry = status["telemetry"]
    assert isinstance(telemetry, dict)
    assert "candidate_trace" not in telemetry
    assert all(
        isinstance(event["telemetry"], dict)
        and "candidate_trace" not in event["telemetry"]
        for event in events
    )

    detail = manager.telemetry_detail(handle.job_id)
    detail_status = detail["status"]
    trace = detail["candidate_trace"]
    assert isinstance(detail_status, dict)
    assert detail_status["job_id"] == handle.job_id
    assert detail_status["lifecycle_state"] == status["lifecycle_state"]
    assert detail_status["terminal_reason"] == status["terminal_reason"]
    assert detail_status["event_sequence"] == status["event_sequence"]
    assert (
        detail_status["latest_snapshot_revision"] == status["latest_snapshot_revision"]
    )
    assert isinstance(trace, dict)
    assert trace["max_entries"] == 128
    assert trace["candidate_index_scope"] == "source_local_only"
    assert isinstance(trace["header"], dict)
    assert isinstance(trace["provenance_status"], dict)
    assert trace["header"]["format_version"] == 3
    assert trace["header"]["qualified_run_provenance"] is None
    assert trace["provenance_status"]["qualification"] == "not_requested"
    assert trace["total_pulls"] >= len(trace["pulls"])
    assert len(trace["pulls"]) <= 128
    assert [event["event_type"] for event in terminal_events(events)] == ["COMPLETED"]

    if trace["pulls"]:
        pull = trace["pulls"][0]
        assert isinstance(pull["source"], str)
        assert "identity" in pull
        assert isinstance(pull["dispositions"], list)

    manager.delete(handle.job_id)


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
    events = manager.events(handle.job_id)
    assert [event["event_type"] for event in terminal_events(events)] == ["COMPLETED"]


def test_manager_solve_does_not_mutate_submitted_solution() -> None:
    plan = WorkerPlan()
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
                    "termination": {"step_count_limit": 4},
                },
            ],
        }
    )

    handle = manager.solve(plan)
    manager.wait(handle.job_id)
    snapshot = manager.snapshot(handle.job_id)
    events = manager.events(handle.job_id)

    assert plan.tasks[0].worker is None
    assert plan.score is None
    assert snapshot.tasks[0].worker == 1
    assert snapshot.score == {"family": "soft", "levels": [0]}
    assert [event["event_type"] for event in terminal_events(events)] == ["COMPLETED"]


@pytest.mark.parametrize(
    ("factory", "expected_score_family"),
    [
        (WorkerPlan, "soft"),
        (Schedule, "hard_soft"),
        (DecimalScoreFamilyPlan, "hard_soft_decimal"),
        (MediumScoreFamilyPlan, "hard_medium_soft"),
    ],
)
def test_manager_preserves_declared_score_family_across_publications(
    factory: Callable[[], object], expected_score_family: str
) -> None:
    direct = Solver.solve(factory())
    direct_score = getattr(direct, "score")
    assert isinstance(direct_score, dict)
    assert direct_score["family"] == expected_score_family

    manager = SolverManager()
    handle = manager.solve(factory())
    status = manager.wait(handle.job_id)
    detail = manager.telemetry_detail(handle.job_id)
    snapshot = manager.snapshot(handle.job_id)
    events = manager.events(handle.job_id)
    completed = [event for event in events if event["event_type"] == "COMPLETED"]

    assert status["lifecycle_state"] == "COMPLETED"
    assert status["best_score"] == direct_score
    assert detail["status"]["best_score"] == direct_score
    assert getattr(snapshot, "score") == direct_score
    assert len(completed) == 1
    assert completed[0]["best_score"] == direct_score
    assert [event["event_type"] for event in terminal_events(events)] == ["COMPLETED"]
    manager.delete(handle.job_id)


def test_manager_assignment_group_zero_step_phase_preserves_callbacks_and_snapshot() -> (
    None
):
    plan = AssignmentSchedule()
    manager = SolverManager(
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "first_fit",
                    "group_name": "shift_nurse_assignment",
                    "termination": {"step_count_limit": 0},
                }
            ]
        }
    )

    handle = manager.solve(plan)
    status = manager.wait(handle.job_id)
    snapshot = manager.snapshot(handle.job_id)

    assert status["lifecycle_state"] == "COMPLETED"
    telemetry = status["telemetry"]
    assert isinstance(telemetry, dict)
    assert telemetry["step_count"] == 0
    assert [shift.nurse for shift in plan.shifts] == [None, None]
    assert plan.score is None
    assert [shift.nurse for shift in snapshot.shifts] == [None, None]
    assert snapshot.score == Solver.analyze(snapshot)
    assert snapshot.score["levels"] == [-2, 0]
    events = manager.events(handle.job_id)
    assert [event["event_type"] for event in terminal_events(events)] == ["COMPLETED"]


def test_manager_assignment_group_missing_group_fails_before_job_starts() -> None:
    manager = SolverManager(
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "group_name": "missing_shift_nurse_assignment",
                }
            ]
        }
    )

    with pytest.raises(RuntimeError, match="no matching assignment scalar group"):
        manager.solve(AssignmentSchedule())


def test_manager_assignment_owned_raw_selector_fails_without_running_wrapper_path() -> (
    None
):
    manager = SolverManager(
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "change_move_selector",
                        "entity_class": "AssignmentShift",
                        "variable_name": "nurse",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        }
    )

    handle = manager.solve(AssignmentSchedule())
    status = manager.wait(handle.job_id)
    events = manager.events(handle.job_id)
    failures = [event for event in events if event["event_type"] == "FAILED"]

    assert status["lifecycle_state"] == "FAILED"
    assert len(failures) == 1
    assert [event["event_type"] for event in terminal_events(events)] == ["FAILED"]
    assert "is assignment-owned" in failures[0]["error"]


def test_manager_failed_event_preserves_callback_traceback() -> None:
    manager = SolverManager()
    handle = manager.solve(BrokenPlan())

    status = manager.wait(handle.job_id)
    events = manager.events(handle.job_id)
    failed = [event for event in events if event["event_type"] == "FAILED"]

    assert status["lifecycle_state"] == "FAILED"
    assert len(failed) == 1
    assert [event["event_type"] for event in terminal_events(events)] == ["FAILED"]
    assert "callback failed" in failed[0]["error"]
    assert "in explode" in failed[0]["error"]


@pytest.mark.parametrize(
    ("plan", "config", "function_name", "message"),
    [
        (
            BrokenGroupProviderPlan,
            BROKEN_GROUP_CONFIG,
            "explode_group_provider",
            "group provider failed",
        ),
        (
            BrokenRepairProviderPlan,
            BROKEN_REPAIR_CONFIG,
            "explode_repair_provider",
            "repair provider failed",
        ),
    ],
)
def test_manager_dynamic_provider_failure_preserves_traceback(
    plan: type[object], config: dict[str, object], function_name: str, message: str
) -> None:
    manager = SolverManager(config)
    handle = manager.solve(plan())

    status = manager.wait(handle.job_id)
    events = manager.events(handle.job_id)
    failed = [event for event in events if event["event_type"] == "FAILED"]

    assert status["lifecycle_state"] == "FAILED"
    assert len(failed) == 1
    assert [event["event_type"] for event in terminal_events(events)] == ["FAILED"]
    assert message in failed[0]["error"]
    assert f"in {function_name}" in failed[0]["error"]
    manager.delete(handle.job_id)
