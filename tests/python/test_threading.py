from concurrent.futures import ThreadPoolExecutor

from tests.python.test_scalar_solving import Schedule
from solverforge import (
    ConstraintFactory,
    HardSoftScore,
    Solver,
    constraint_provider,
    planning_entity,
    planning_solution,
    planning_variable,
)


def test_concurrent_python_solves_do_not_share_solution_state() -> None:
    with ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(lambda _: Solver.solve(Schedule()), range(2)))
    assert results[0] is not results[1]


@planning_entity
class ParallelVisit:
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self, preferred: int) -> None:
        self.preferred = preferred
        self.nurse = None


@constraint_provider
def parallel_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(ParallelVisit)
        .filter(lambda visit: visit.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("unassigned"),
        factory.for_each(ParallelVisit)
        .filter(lambda visit: visit.nurse is not None and visit.nurse != visit.preferred)
        .penalize(HardSoftScore.ONE_SOFT)
        .named("not preferred"),
    ]


@planning_solution(score=HardSoftScore, constraints=parallel_constraints)
class ParallelPlan:
    visits: list[ParallelVisit]

    def __init__(self) -> None:
        self.visits = [ParallelVisit((index * 7 + 3) % 5) for index in range(30)]
        self.nurses = [0, 1, 2, 3, 4]
        self.score = None


def _solve_with_threads(count: int) -> ParallelPlan:
    return Solver.solve(
        ParallelPlan(),
        {
            "random_seed": 7,
            "move_thread_count": {"count": count},
            "phases": [
                {"type": "construction_heuristic"},
                {"type": "local_search", "termination": {"step_count_limit": 60}},
            ],
        },
    )


def test_parallel_move_evaluation_matches_sequential_at_fixed_work() -> None:
    plans = [_solve_with_threads(count) for count in (1, 2, 4)]
    assignments = [[visit.nurse for visit in plan.visits] for plan in plans]
    assert assignments[0] == assignments[1] == assignments[2]
    assert plans[0].score == plans[1].score == plans[2].score
