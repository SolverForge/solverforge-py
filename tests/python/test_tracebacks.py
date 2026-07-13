import traceback

import pytest

from solverforge import (
    ConstraintFactory,
    HardSoftScore,
    SoftScore,
    Solver,
    conflict_repair,
    constraint_provider,
    planning_entity,
    planning_solution,
    planning_variable,
    scalar_group,
)


@planning_entity
class BrokenTask:
    worker = planning_variable(value_range_provider="workers", allows_unassigned=True)

    def __init__(self) -> None:
        self.worker = None


@constraint_provider
def constraints(factory: ConstraintFactory):
    def explode(_task: BrokenTask) -> bool:
        raise ValueError("callback failed")

    return [factory.for_each(BrokenTask).filter(explode).penalize(1).named("broken")]


@planning_solution(constraints=constraints)
class BrokenPlan:
    tasks: list[BrokenTask]

    def __init__(self) -> None:
        self.tasks = [BrokenTask()]
        self.workers = [0]
        self.score = None


def test_callback_traceback_surfaces() -> None:
    with pytest.raises(ValueError, match="callback failed") as exc_info:
        Solver.solve(BrokenPlan())
    frames = traceback.extract_tb(exc_info.value.__traceback__)
    assert any(frame.name == "explode" for frame in frames)


def test_analyze_callback_traceback_surfaces() -> None:
    with pytest.raises(ValueError, match="callback failed") as exc_info:
        Solver.analyze(BrokenPlan())
    frames = traceback.extract_tb(exc_info.value.__traceback__)
    assert any(frame.name == "explode" for frame in frames)


@planning_entity
class ProviderTask:
    worker = planning_variable(value_range_provider="workers")

    def __init__(self, worker: int, target: int) -> None:
        self.worker = worker
        self.target = target


@constraint_provider
def group_provider_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(ProviderTask)
        .filter(lambda task: task.worker != task.target)
        .penalize(SoftScore.of(1))
        .named("provider target"),
    ]


@scalar_group("broken_provider_group")
def explode_group_provider(solution, limits):
    del solution, limits
    raise ValueError("group provider failed")


BROKEN_GROUP_CONFIG = {
    "phases": [
        {
            "type": "local_search",
            "local_search_type": "acceptor_forager",
            "move_selector": {
                "type": "grouped_scalar_move_selector",
                "group_name": "broken_provider_group",
            },
            "acceptor": {"type": "hill_climbing"},
            "forager": {"type": "best_score"},
            "termination": {"step_count_limit": 1},
        }
    ]
}


@planning_solution(
    score=SoftScore,
    constraints=group_provider_constraints,
    scalar_groups=[explode_group_provider],
)
class BrokenGroupProviderPlan:
    tasks: list[ProviderTask]

    def __init__(self) -> None:
        self.tasks = [ProviderTask(0, 1)]
        self.workers = [0, 1]
        self.score = None


@constraint_provider
def repair_provider_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(ProviderTask)
        .filter(lambda task: task.worker != task.target)
        .penalize(HardSoftScore.ONE_HARD)
        .named("provider repair"),
    ]


@conflict_repair("provider repair")
def explode_repair_provider(solution, limits):
    del solution, limits
    raise ValueError("repair provider failed")


BROKEN_REPAIR_CONFIG = {
    "phases": [
        {
            "type": "local_search",
            "local_search_type": "acceptor_forager",
            "move_selector": {
                "type": "conflict_repair_move_selector",
                "constraints": ["provider repair"],
            },
            "acceptor": {"type": "hill_climbing"},
            "forager": {"type": "best_score"},
            "termination": {"step_count_limit": 1},
        }
    ]
}


@planning_solution(
    score=HardSoftScore,
    constraints=repair_provider_constraints,
    conflict_repairs=[explode_repair_provider],
)
class BrokenRepairProviderPlan:
    tasks: list[ProviderTask]

    def __init__(self) -> None:
        self.tasks = [ProviderTask(0, 1)]
        self.workers = [0, 1]
        self.score = None


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
def test_dynamic_provider_callback_traceback_surfaces(
    plan: type[object], config: dict[str, object], function_name: str, message: str
) -> None:
    with pytest.raises(ValueError, match=message) as exc_info:
        Solver.solve(plan(), config)
    frames = traceback.extract_tb(exc_info.value.__traceback__)
    assert any(frame.name == function_name for frame in frames)
