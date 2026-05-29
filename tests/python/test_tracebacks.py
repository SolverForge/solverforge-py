import pytest

from solverforge import ConstraintFactory, Solver, constraint_provider
from solverforge import planning_entity, planning_solution, planning_variable


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
    with pytest.raises(Exception, match="callback failed"):
        Solver.solve(BrokenPlan())


def test_analyze_callback_traceback_surfaces() -> None:
    with pytest.raises(Exception, match="callback failed"):
        Solver.analyze(BrokenPlan())
