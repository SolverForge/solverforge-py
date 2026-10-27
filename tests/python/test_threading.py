import os
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import pytest

from tests.python.test_scalar_solving import Schedule
from solverforge import (
    ConstraintFactory,
    HardMediumSoftScore,
    HardSoftDecimalScore,
    HardSoftScore,
    SoftScore,
    Solver,
    SolverConfig,
    SolverManager,
    constraint_provider,
    planning_entity,
    planning_list_variable,
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


def _fixed_work_config(count: int) -> dict[str, Any]:
    return {
        "random_seed": 7,
        "move_thread_count": {"count": count},
        "phases": [
            {"type": "construction_heuristic"},
            {"type": "local_search", "termination": {"step_count_limit": 60}},
        ],
    }


def _solve_with_threads(count: int) -> ParallelPlan:
    return Solver.solve(ParallelPlan(), _fixed_work_config(count))


def test_parallel_move_evaluation_matches_sequential_at_fixed_work() -> None:
    plans = [_solve_with_threads(count) for count in (1, 2, 4)]
    assignments = [[visit.nurse for visit in plan.visits] for plan in plans]
    assert assignments[0] == assignments[1] == assignments[2]
    assert plans[0].score == plans[1].score == plans[2].score


@pytest.mark.parametrize(
    "toml_value", ['"none"', '"auto"', "{ count = 2 }"], ids=["none", "auto", "count"]
)
def test_move_thread_count_from_toml_reaches_the_solver(toml_value: str) -> None:
    config = SolverConfig.from_toml(f"""
random_seed = 7
move_thread_count = {toml_value}

[[phases]]
type = "construction_heuristic"

[[phases]]
type = "local_search"
termination = {{ step_count_limit = 60 }}
""")
    assert "move_thread_count" in config.to_dict()

    plan = Solver.solve(ParallelPlan(), config)
    sequential = _solve_with_threads(1)
    assert [visit.nurse for visit in plan.visits] == [visit.nurse for visit in sequential.visits]
    assert plan.score == sequential.score


def _retained_telemetry(count: int) -> dict[str, Any]:
    manager = SolverManager(_fixed_work_config(count))
    handle = manager.solve(ParallelPlan())
    status = manager.wait(handle.job_id)
    assert status["lifecycle_state"] == "COMPLETED"
    telemetry = status["telemetry"]
    assert isinstance(telemetry, dict)
    return telemetry


@pytest.mark.skipif(
    (os.process_cpu_count() or 1) < 2, reason="parallel evaluation needs two Rayon threads"
)
def test_retained_telemetry_reports_speculative_work_per_phase() -> None:
    sequential = _retained_telemetry(1)
    parallel = _retained_telemetry(2)

    for telemetry in (sequential, parallel):
        assert "phase" not in telemetry
        phases = telemetry["phases"]
        assert [phase["phase_type"] for phase in phases] == [
            "Construction Heuristic",
            "Local Search",
        ]
        assert [phase["phase_index"] for phase in phases] == [0, 1]
        assert sum(phase["moves_evaluated"] for phase in phases) == telemetry["moves_evaluated"]
        assert telemetry["moves_speculative"] == sum(phase["moves_speculative"] for phase in phases)
        assert phases[0]["moves_speculative"] == 0

    # Same trajectory, so the same decided work; only parallel evaluation
    # evaluates candidates ahead of the decision point and discards them.
    assert parallel["moves_evaluated"] == sequential["moves_evaluated"]
    assert sequential["moves_speculative"] == 0
    assert parallel["phases"][1]["moves_speculative"] > 0


# Each family's declared name and its (unassigned, not preferred) penalties.
FAMILY_WEIGHTS: dict[type, tuple[str, object, object]] = {
    SoftScore: ("soft", SoftScore.of(10), SoftScore.ONE_SOFT),
    HardSoftScore: ("hard_soft", HardSoftScore.ONE_HARD, HardSoftScore.ONE_SOFT),
    HardSoftDecimalScore: (
        "hard_soft_decimal",
        HardSoftDecimalScore.ONE_HARD,
        HardSoftDecimalScore.ONE_SOFT,
    ),
    HardMediumSoftScore: (
        "hard_medium_soft",
        HardMediumSoftScore.ONE_HARD,
        HardMediumSoftScore.ONE_MEDIUM,
    ),
}


def _family_plan(score_cls: type) -> type:
    _, unassigned, not_preferred = FAMILY_WEIGHTS[score_cls]

    @planning_entity
    class FamilyVisit:
        nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

        def __init__(self, preferred: int) -> None:
            self.preferred = preferred
            self.nurse = None

    @constraint_provider
    def family_constraints(factory: ConstraintFactory):
        return [
            factory.for_each(FamilyVisit)
            .filter(lambda visit: visit.nurse is None)
            .penalize(unassigned)
            .named("unassigned"),
            factory.for_each(FamilyVisit)
            .filter(lambda visit: visit.nurse is not None and visit.nurse != visit.preferred)
            .penalize(not_preferred)
            .named("not preferred"),
        ]

    @planning_solution(score=score_cls, constraints=family_constraints)
    class FamilyPlan:
        visits: list[FamilyVisit]

        def __init__(self) -> None:
            self.visits = [FamilyVisit((index * 7 + 3) % 5) for index in range(12)]
            self.nurses = [0, 1, 2, 3, 4]
            self.score = None

    return FamilyPlan


@pytest.mark.parametrize("score_cls", list(FAMILY_WEIGHTS), ids=lambda cls: cls.__name__)
def test_parallel_evaluation_keeps_every_score_family(score_cls: type) -> None:
    plan_cls = _family_plan(score_cls)
    plans = [Solver.solve(plan_cls(), _fixed_work_config(count)) for count in (1, 2, 4)]

    assignments = [[visit.nurse for visit in plan.visits] for plan in plans]
    assert assignments[0] == assignments[1] == assignments[2]
    assert plans[0].score == plans[1].score == plans[2].score
    family = FAMILY_WEIGHTS[score_cls][0]
    assert all(plan.score["family"] == family for plan in plans)


@planning_entity
class ParallelRoute:
    visits = planning_list_variable(element_collection="stops")

    def __init__(self) -> None:
        self.visits: list[int] = []


def _route_cost(route: ParallelRoute) -> SoftScore:
    # Position-dependent cost, so the order inside a route matters as well
    # as which route owns a stop.
    return SoftScore.of(
        sum((stop * (position + 3)) % 7 for position, stop in enumerate(route.visits))
    )


@constraint_provider
def parallel_route_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(ParallelRoute).penalize(_route_cost).named("route cost"),
        factory.for_each(ParallelRoute)
        .penalize(lambda route: SoftScore.of(len(route.visits) ** 2))
        .named("balance"),
    ]


@planning_solution(score=SoftScore, constraints=parallel_route_constraints)
class ParallelRoutePlan:
    routes: list[ParallelRoute]

    def __init__(self) -> None:
        self.routes = [ParallelRoute() for _ in range(3)]
        self.stops = list(range(14))
        self.score = None


def test_parallel_list_evaluation_matches_sequential_at_fixed_work() -> None:
    plans = [Solver.solve(ParallelRoutePlan(), _fixed_work_config(count)) for count in (1, 2, 4)]

    routes = [[route.visits for route in plan.routes] for plan in plans]
    assert routes[0] == routes[1] == routes[2]
    assert plans[0].score == plans[1].score == plans[2].score
    assert sorted(stop for route in routes[0] for stop in route) == list(range(14))
