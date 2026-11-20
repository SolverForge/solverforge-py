"""`score_director.constraint_provider` selects a solution's constraint provider by name."""

import pytest

from solverforge import (
    SoftScore,
    Solver,
    SolverManager,
    constraint_provider,
    planning_entity,
    planning_solution,
    planning_variable,
)
from solverforge.constraints import ConstraintFactory
from solverforge.model import build_schema


@constraint_provider
def prefer_worker_one(factory: ConstraintFactory):
    return [
        factory.for_each(ProviderTask)
        .filter(lambda task: task.worker != 1)
        .penalize(SoftScore.of(1))
        .named("prefer worker one")
    ]


@planning_entity
class ProviderTask:
    worker = planning_variable(value_range_provider="workers")

    def __init__(self) -> None:
        self.worker: int | None = None


@planning_solution(score=SoftScore, constraints=prefer_worker_one)
class ProviderPlan:
    provider_tasks: list[ProviderTask]

    def __init__(self) -> None:
        self.provider_tasks = [ProviderTask(), ProviderTask()]
        self.workers = [0, 1, 2]
        self.score = None


@planning_solution(score=SoftScore)
class UnconstrainedPlan:
    provider_tasks: list[ProviderTask]

    def __init__(self) -> None:
        self.provider_tasks = [ProviderTask()]
        self.workers = [0, 1]
        self.score = None


def provider_config(name: str) -> dict[str, object]:
    return {
        "termination": {"step_count_limit": 20},
        "score_director": {"constraint_provider": name},
    }


def test_schema_names_the_provider_by_its_function_name() -> None:
    assert build_schema(ProviderPlan())["constraint_provider"] == "prefer_worker_one"
    assert build_schema(UnconstrainedPlan())["constraint_provider"] is None


def test_named_provider_scores_the_solve() -> None:
    plan = Solver.solve(ProviderPlan(), provider_config("prefer_worker_one"))

    assert [task.worker for task in plan.provider_tasks] == [1, 1]
    assert plan.score == {"family": "soft", "levels": [0]}
    assert plan.score == Solver.analyze(plan)


def test_unknown_provider_fails_before_solving() -> None:
    with pytest.raises(
        RuntimeError,
        match="unknown constraint provider `missing`; declared providers: prefer_worker_one",
    ):
        Solver.solve(ProviderPlan(), provider_config("missing"))


def test_solution_without_constraints_declares_no_provider() -> None:
    with pytest.raises(RuntimeError, match="unknown constraint provider `prefer_worker_one`"):
        Solver.solve(UnconstrainedPlan(), provider_config("prefer_worker_one"))

    plan = Solver.solve(UnconstrainedPlan(), {"termination": {"step_count_limit": 5}})
    assert plan.score == {"family": "soft", "levels": [0]}


def test_retained_job_fails_on_unknown_provider() -> None:
    manager = SolverManager(provider_config("missing"))
    handle = manager.solve(ProviderPlan())

    status = manager.wait(handle.job_id)

    assert status["lifecycle_state"] == "FAILED"
    failures = [event for event in manager.events(handle.job_id) if event["event_type"] == "FAILED"]
    assert [event["error"] for event in failures] == [
        "unknown constraint provider `missing`; declared providers: prefer_worker_one"
    ]
