from examples.list_tsp import Tsp
import pytest

from solverforge import (
    ConstraintFactory,
    SoftScore,
    Solver,
    constraint_provider,
    planning_entity,
    planning_list_variable,
    planning_solution,
    planning_variable,
)
from solverforge.model import build_schema


def test_list_variable_metadata_is_collected() -> None:
    schema = build_schema(Tsp())
    field = schema["entities"][0]["fields"][0]
    assert field["kind"] == "planning_list_variable"
    assert field["element_collection"] == "visit_values"


def test_dynamic_list_assignment_solves_python_model() -> None:
    tsp = Solver.solve(Tsp())
    assert tsp.tours[0].visits == [3, 2, 1, 0]


@planning_entity
class Route:
    visits = planning_list_variable(element_collection="visit_values")

    def __init__(self, visits: list[int] | None = None) -> None:
        self.visits = visits or []


@constraint_provider
def prefer_ordered_visits(factory: ConstraintFactory):
    return [
        factory.for_each(Route)
        .filter(lambda route: route.visits != [0, 1])
        .penalize(SoftScore.of(10))
        .named("prefer ordered visits")
    ]


@planning_solution(score=SoftScore, constraints=prefer_ordered_visits)
class RoutePlan:
    routes: list[Route]

    def __init__(self) -> None:
        self.routes = [Route([1, 0])]
        self.visit_values = [0, 1]
        self.score = None


def test_dynamic_list_local_search_uses_upstream_move_selector() -> None:
    plan = Solver.solve(
        RoutePlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "variable_neighborhood_descent",
                    "neighborhoods": [
                        {
                            "type": "list_change_move_selector",
                            "entity_class": "Route",
                            "variable_name": "visits",
                        }
                    ],
                    "termination": {"step_count_limit": 4},
                }
            ]
        },
    )

    assert plan.routes[0].visits == [0, 1]
    assert all(level == 0 for level in plan.score["levels"])


@constraint_provider
def prefer_sorted_route(factory: ConstraintFactory):
    return [
        factory.for_each(Route)
        .filter(lambda route: route.visits != sorted(route.visits))
        .penalize(SoftScore.of(1))
        .named("prefer sorted route")
    ]


@planning_solution(score=SoftScore, constraints=prefer_sorted_route)
class FourVisitRoutePlan:
    routes: list[Route]

    def __init__(self) -> None:
        self.routes = [Route([3, 2, 1, 0])]
        self.visit_values = [0, 1, 2, 3]
        self.score = None


LIST_MOVE_SELECTORS = [
    {
        "type": "list_change_move_selector",
        "entity_class": "Route",
        "variable_name": "visits",
    },
    {
        "type": "nearby_list_change_move_selector",
        "entity_class": "Route",
        "variable_name": "visits",
        "max_nearby": 4,
    },
    {
        "type": "list_swap_move_selector",
        "entity_class": "Route",
        "variable_name": "visits",
    },
    {
        "type": "nearby_list_swap_move_selector",
        "entity_class": "Route",
        "variable_name": "visits",
        "max_nearby": 4,
    },
    {
        "type": "sublist_change_move_selector",
        "entity_class": "Route",
        "variable_name": "visits",
        "min_sublist_size": 1,
        "max_sublist_size": 2,
    },
    {
        "type": "sublist_swap_move_selector",
        "entity_class": "Route",
        "variable_name": "visits",
        "min_sublist_size": 1,
        "max_sublist_size": 2,
    },
    {
        "type": "list_reverse_move_selector",
        "entity_class": "Route",
        "variable_name": "visits",
    },
    {
        "type": "k_opt_move_selector",
        "entity_class": "Route",
        "variable_name": "visits",
        "k": 2,
        "min_segment_len": 1,
        "max_nearby": 0,
    },
    {
        "type": "list_ruin_move_selector",
        "entity_class": "Route",
        "variable_name": "visits",
        "min_ruin_count": 1,
        "max_ruin_count": 2,
        "moves_per_step": 4,
    },
]


@pytest.mark.parametrize("move_selector", LIST_MOVE_SELECTORS)
def test_dynamic_list_local_search_supports_every_list_move_selector(
    move_selector: dict[str, object],
) -> None:
    plan = Solver.solve(
        FourVisitRoutePlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": move_selector,
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 2},
                }
            ]
        },
    )

    assert sorted(plan.routes[0].visits) == [0, 1, 2, 3]
    assert plan.score is not None


@planning_solution(score=SoftScore, constraints=prefer_sorted_route)
class TwoRoutePlan:
    routes: list[Route]

    def __init__(self) -> None:
        self.routes = [Route([1, 0]), Route([3, 2])]
        self.visit_values = [0, 1, 2, 3]
        self.score = None


def test_dynamic_cartesian_selector_composes_list_moves() -> None:
    plan = Solver.solve(
        TwoRoutePlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "cartesian_product_move_selector",
                        "selectors": [
                            {
                                "type": "list_reverse_move_selector",
                                "entity_class": "Route",
                                "variable_name": "visits",
                            },
                            {
                                "type": "list_reverse_move_selector",
                                "entity_class": "Route",
                                "variable_name": "visits",
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [route.visits for route in plan.routes] == [[0, 1], [2, 3]]
    assert all(level == 0 for level in plan.score["levels"])


@planning_entity
class MixedTask:
    worker = planning_variable(value_range_provider="workers")

    def __init__(self, worker: int | None = None) -> None:
        self.worker = worker


@constraint_provider
def prefer_route_and_worker(factory: ConstraintFactory):
    return [
        factory.for_each(Route)
        .filter(lambda route: route.visits != sorted(route.visits))
        .penalize(SoftScore.of(10))
        .named("route sorted"),
        factory.for_each(MixedTask)
        .filter(lambda task: task.worker != 1)
        .penalize(SoftScore.of(10))
        .named("worker one"),
    ]


@planning_solution(score=SoftScore, constraints=prefer_route_and_worker)
class MixedCartesianPlan:
    routes: list[Route]
    tasks: list[MixedTask]

    def __init__(self) -> None:
        self.routes = [Route([1, 0])]
        self.tasks = [MixedTask(0)]
        self.visit_values = [0, 1]
        self.workers = [0, 1]
        self.score = None


def test_dynamic_cartesian_selector_composes_scalar_and_list_moves() -> None:
    plan = Solver.solve(
        MixedCartesianPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "cartesian_product_move_selector",
                        "selectors": [
                            {
                                "type": "list_reverse_move_selector",
                                "entity_class": "Route",
                                "variable_name": "visits",
                            },
                            {
                                "type": "change_move_selector",
                                "entity_class": "MixedTask",
                                "variable_name": "worker",
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert plan.routes[0].visits == [0, 1]
    assert [task.worker for task in plan.tasks] == [1]
    assert all(level == 0 for level in plan.score["levels"])


def test_dynamic_union_selector_composes_scalar_and_list_children() -> None:
    plan = Solver.solve(
        MixedCartesianPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "union_move_selector",
                        "selectors": [
                            {
                                "type": "list_reverse_move_selector",
                                "entity_class": "Route",
                                "variable_name": "visits",
                            },
                            {
                                "type": "change_move_selector",
                                "entity_class": "MixedTask",
                                "variable_name": "worker",
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 2},
                }
            ]
        },
    )

    assert plan.routes[0].visits == [0, 1]
    assert [task.worker for task in plan.tasks] == [1]
    assert all(level == 0 for level in plan.score["levels"])
