from examples.list_tsp import Tsp
import pytest

from solverforge import (
    ConstraintFactory,
    HardSoftScore,
    SoftScore,
    Solver,
    constraint_provider,
    planning_entity,
    planning_list_variable,
    planning_solution,
    planning_variable,
    shadow_variable_updates,
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


def owned_visit_owner(solution: object, element: int) -> int:
    del solution
    return int(element) % 2


@planning_entity
class OwnedRoute:
    visits = planning_list_variable(
        element_collection="owned_visit_values",
        element_owner=owned_visit_owner,
    )

    def __init__(self, visits: list[int] | None = None) -> None:
        self.visits = visits or []


@constraint_provider
def reward_non_empty_owned_routes(factory: ConstraintFactory):
    return [
        factory.for_each(OwnedRoute)
        .filter(lambda route: bool(route.visits))
        .reward(SoftScore.of(1))
        .named("non-empty owned route")
    ]


@planning_solution(score=SoftScore, constraints=reward_non_empty_owned_routes)
class OwnedRoutePlan:
    owned_routes: list[OwnedRoute]

    def __init__(self) -> None:
        self.owned_routes = [OwnedRoute(), OwnedRoute()]
        self.owned_visit_values = [0, 1, 2, 3]
        self.score = None


def test_dynamic_list_cheapest_insertion_respects_element_owner() -> None:
    plan = Solver.solve(
        OwnedRoutePlan(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_cheapest_insertion",
                    "entity_class": "OwnedRoute",
                    "variable_name": "visits",
                }
            ]
        },
    )

    assigned = sorted(visit for route in plan.owned_routes for visit in route.visits)
    assert assigned == [0, 1, 2, 3]
    for owner_index, route in enumerate(plan.owned_routes):
        assert route.visits
        assert all(visit % 2 == owner_index for visit in route.visits)


@planning_entity
class OwnedSearchRoute:
    visits = planning_list_variable(
        element_collection="owned_search_visit_values",
        element_owner=owned_visit_owner,
    )

    def __init__(self, route_index: int, visits: list[int] | None = None) -> None:
        self.route_index = route_index
        self.visits = visits or []


@constraint_provider
def prefer_owned_search_route_without_odd_visit(factory: ConstraintFactory):
    return [
        factory.for_each(OwnedSearchRoute)
        .filter(lambda route: route.route_index == 1 and 1 in route.visits)
        .penalize(SoftScore.of(100))
        .named("owned search route avoids odd visit")
    ]


@planning_solution(
    score=SoftScore, constraints=prefer_owned_search_route_without_odd_visit
)
class OwnedSearchRoutePlan:
    owned_search_routes: list[OwnedSearchRoute]

    def __init__(self) -> None:
        self.owned_search_routes = [
            OwnedSearchRoute(0, [0]),
            OwnedSearchRoute(1, [1]),
        ]
        self.owned_search_visit_values = [0, 1]
        self.score = None


def test_dynamic_list_local_search_respects_element_owner() -> None:
    plan = Solver.solve(
        OwnedSearchRoutePlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "list_change_move_selector",
                        "entity_class": "OwnedSearchRoute",
                        "variable_name": "visits",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [route.visits for route in plan.owned_search_routes] == [[0], [1]]
    assert plan.score == {"family": "soft", "levels": [-100]}


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


@constraint_provider
def penalize_unassigned_visits(factory: ConstraintFactory):
    return [
        factory.for_each_unassigned_element(Route, "visits")
        .penalize(SoftScore.of(10))
        .named("unassigned visits")
    ]


@planning_solution(score=SoftScore, constraints=penalize_unassigned_visits)
class UnassignedRoutePlan:
    routes: list[Route]

    def __init__(self) -> None:
        self.routes = [Route([0]), Route([])]
        self.visit_values = [0, 1, 2]
        self.score = None


def test_dynamic_list_unassigned_element_scores_missing_values() -> None:
    plan = UnassignedRoutePlan()

    score = Solver.analyze(plan)

    assert score == {"family": "soft", "levels": [-20]}
    assert plan.score == score


def test_dynamic_list_unassigned_element_construction_inserts_all_values() -> None:
    plan = Solver.solve(
        UnassignedRoutePlan(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_cheapest_insertion",
                    "entity_class": "Route",
                    "variable_name": "visits",
                }
            ]
        },
    )

    assigned = sorted(visit for route in plan.routes for visit in route.visits)

    assert assigned == [0, 1, 2]
    assert plan.score == {"family": "soft", "levels": [0]}


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


def shadow_total(solution: object, entity_index: int) -> dict[str, int]:
    route = solution.shadow_routes[entity_index]
    return {"route_total": sum(route.visits)}


@planning_entity
class ShadowRoute:
    visits = planning_list_variable(element_collection="visit_values")

    def __init__(self, visits: list[int] | None = None) -> None:
        self.visits = visits or []
        self.route_total = 0


@constraint_provider
def prefer_shadow_total(factory: ConstraintFactory):
    return [
        factory.for_each(ShadowRoute)
        .filter(lambda route: bool(route.visits) and route.route_total != 3)
        .penalize(HardSoftScore.of_hard(1))
        .named("shadow total")
    ]


@planning_solution(
    score=HardSoftScore,
    constraints=prefer_shadow_total,
    shadow_updates=shadow_variable_updates(
        list_owner="shadow_routes",
        post_update_listener=shadow_total,
    ),
)
class ShadowRoutePlan:
    shadow_routes: list[ShadowRoute]

    def __init__(self) -> None:
        self.shadow_routes = [ShadowRoute([1]), ShadowRoute([2])]
        self.visit_values = [1, 2]
        self.score = None


def test_dynamic_list_moves_refresh_python_shadow_fields() -> None:
    plan = Solver.solve(
        ShadowRoutePlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "list_change_move_selector",
                        "entity_class": "ShadowRoute",
                        "variable_name": "visits",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [route.route_total for route in plan.shadow_routes] in ([3, 0], [0, 3])
    assert plan.score["levels"][0] == 0


def cvrp_route_distance(
    solution: object,
    entity_index: int,
    from_element: int,
    to_element: int,
) -> int:
    vehicle = solution.cvrp_routes[entity_index]
    return int(vehicle.distance_matrix[from_element][to_element])


def cvrp_route_depot(solution: object, entity_index: int) -> int:
    return int(solution.cvrp_routes[entity_index].depot)


def cvrp_route_feasible(solution: object, entity_index: int, route: list[int]) -> bool:
    vehicle = solution.cvrp_routes[entity_index]
    return sum(vehicle.demands[visit] for visit in route) <= vehicle.capacity


@planning_entity
class CvrpRoute:
    visits = planning_list_variable(
        element_collection="visit_values",
        route_depot=cvrp_route_depot,
        route_distance=cvrp_route_distance,
        route_feasible=cvrp_route_feasible,
    )

    def __init__(
        self, *, depot: int, capacity: int, visits: list[int] | None = None
    ) -> None:
        self.depot = depot
        self.capacity = capacity
        self.demands = [1, 1, 1]
        self.distance_matrix = [
            [0, 8, 2, 2],
            [8, 0, 2, 2],
            [2, 2, 0, 2],
            [2, 2, 2, 0],
        ]
        self.visits = visits or []


@constraint_provider
def cvrp_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(CvrpRoute)
        .filter(lambda route: bool(route.visits))
        .reward(HardSoftScore.of_soft(1))
        .named("non-empty route")
    ]


@planning_solution(score=HardSoftScore, constraints=cvrp_constraints)
class CvrpPlan:
    cvrp_routes: list[CvrpRoute]

    def __init__(self) -> None:
        self.cvrp_routes = [
            CvrpRoute(depot=3, capacity=2),
            CvrpRoute(depot=3, capacity=2),
        ]
        self.visit_values = [0, 1, 2]
        self.score = None


def test_dynamic_cvrp_route_hooks_bind_list_clarke_wright_and_k_opt() -> None:
    plan = Solver.solve(
        CvrpPlan(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_clarke_wright",
                    "entity_class": "CvrpRoute",
                    "variable_name": "visits",
                },
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_k_opt",
                    "entity_class": "CvrpRoute",
                    "variable_name": "visits",
                    "k": 2,
                },
            ]
        },
    )

    assigned = sorted(visit for route in plan.cvrp_routes for visit in route.visits)
    assert assigned == [0, 1, 2]
    assert all(len(route.visits) <= route.capacity for route in plan.cvrp_routes)
