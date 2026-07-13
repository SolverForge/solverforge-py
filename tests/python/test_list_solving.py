from examples.list_tsp import Tsp
import pytest

from solverforge import (
    CapacityRouteFeasibility,
    ConstraintFactory,
    EntityCallback,
    HardSoftScore,
    ListRouteHooks,
    ListSavingsHooks,
    RowField,
    SoftScore,
    SolutionCallback,
    Solver,
    SolverManager,
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


def list_order_key(solution: object, element: int) -> int:
    del solution
    return int(element)


def list_precedence_duration(solution: object, element: int) -> int:
    del solution
    del element
    return 1


def list_precedence_successors(solution: object, element: int) -> list[int]:
    del solution
    return [int(element) + 1] if element < 2 else []


def route_depot(route: object) -> int:
    return int(route.depot)


def route_distance(route: object, from_element: int, to_element: int) -> int:
    return int(route.distance_matrix[from_element][to_element])


def route_feasible(route: object, visits: list[int]) -> bool:
    return sum(route.demands[visit] for visit in visits) <= route.capacity


ROW_ROUTE_HOOKS = ListRouteHooks(
    depot=RowField("depot"),
    distance=RowField("distance_matrix"),
    feasible=CapacityRouteFeasibility(
        capacity=RowField("capacity"),
        demand=RowField("demands"),
    ),
)
ROW_SAVINGS_HOOKS = ListSavingsHooks(
    depot=RowField("depot"),
    metric_class=RowField("metric_class"),
    distance=RowField("distance_matrix"),
    feasible=CapacityRouteFeasibility(
        capacity=RowField("capacity"),
        demand=RowField("demands"),
    ),
)


@planning_entity
class MetadataRoute:
    visits = planning_list_variable(
        element_collection="metadata_visit_values",
        construction_element_order_key=list_order_key,
        precedence_duration=list_precedence_duration,
        precedence_successors=list_precedence_successors,
        route=ROW_ROUTE_HOOKS,
        savings=ROW_SAVINGS_HOOKS,
        cross_position_distance=RowField("distance_matrix"),
        intra_position_distance=RowField("distance_matrix"),
    )


@planning_solution(score=SoftScore)
class MetadataPlan:
    metadata_routes: list[MetadataRoute]

    def __init__(self) -> None:
        self.metadata_routes = [MetadataRoute()]
        self.metadata_visit_values = [0, 1, 2]
        self.score = None


def test_list_variable_extended_metadata_is_collected() -> None:
    schema = build_schema(MetadataPlan())
    field = schema["entities"][0]["fields"][0]
    assert field["kind"] == "planning_list_variable"
    assert callable(field["construction_element_order_key"])
    assert callable(field["precedence_duration"])
    assert callable(field["precedence_successors"])
    metadata = field["list_metadata"]
    assert metadata["route"] == {
        "depot": {"kind": "row", "field": "depot"},
        "distance": {"kind": "row", "field": "distance_matrix"},
        "feasible": {
            "kind": "capacity",
            "capacity": {"kind": "row", "field": "capacity"},
            "demand": {"kind": "row", "field": "demands"},
        },
    }
    assert metadata["savings"] == {
        "depot": {"kind": "row", "field": "depot"},
        "metric_class": {"kind": "row", "field": "metric_class"},
        "distance": {"kind": "row", "field": "distance_matrix"},
        "feasible": {
            "kind": "capacity",
            "capacity": {"kind": "row", "field": "capacity"},
            "demand": {"kind": "row", "field": "demands"},
        },
    }
    assert metadata["cross_position_distance"] == {
        "kind": "row",
        "field": "distance_matrix",
    }
    assert metadata["intra_position_distance"] == {
        "kind": "row",
        "field": "distance_matrix",
    }


def test_dynamic_list_assignment_solves_python_model() -> None:
    tsp = Solver.solve(Tsp())
    assert tsp.tours[0].visits == [3, 2, 1, 0]


def test_duplicate_list_element_stream_is_rejected_before_solver_work() -> None:
    tsp = Tsp()
    tsp.visit_values = [0, 0]

    with pytest.raises(
        RuntimeError,
        match=r"duplicate element value 0 at source indexes 0 and 1",
    ):
        Solver.analyze(tsp)


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


@planning_entity
class FieldOwnedRoute:
    visits = planning_list_variable(
        element_collection="field_owned_visit_values",
        element_owner="field_owned_visit_owners",
    )

    def __init__(self) -> None:
        self.visits: list[int] = []


@planning_solution(score=SoftScore)
class FieldOwnedRoutePlan:
    field_owned_routes: list[FieldOwnedRoute]

    def __init__(self) -> None:
        self.field_owned_routes = [FieldOwnedRoute(), FieldOwnedRoute()]
        self.field_owned_visit_values = [0, 1, 2, 3]
        self.field_owned_visit_owners = [0, 1, 0, 1]
        self.score = None


def test_list_element_metadata_reports_solution_level_sequence_source() -> None:
    with pytest.raises(
        TypeError,
        match="element_owner must be a callable or solution-level sequence name",
    ):
        planning_list_variable(
            element_collection="values",
            element_owner=42,  # type: ignore[arg-type]
        )


@planning_entity
class ValidatedFieldMetadataRoute:
    visits = planning_list_variable(
        element_collection="validated_field_metadata_values",
        element_owner="validated_field_metadata_owners",
        construction_element_order_key="validated_field_metadata_order",
        precedence_duration="validated_field_metadata_durations",
        precedence_successors="validated_field_metadata_successors",
    )

    def __init__(self) -> None:
        self.visits: list[int] = []


@planning_solution(score=SoftScore)
class ValidatedFieldMetadataPlan:
    routes: list[ValidatedFieldMetadataRoute]

    def __init__(self) -> None:
        self.routes = [ValidatedFieldMetadataRoute(), ValidatedFieldMetadataRoute()]
        self.validated_field_metadata_values = [0, 1, 2]
        self.validated_field_metadata_owners = [None, 0, 1]
        self.validated_field_metadata_order = [2, 0, 1]
        self.validated_field_metadata_durations = [1, 2, 3]
        self.validated_field_metadata_successors = [[1], [2], []]
        self.score = None


def test_field_backed_list_metadata_accepts_valid_solution_indexed_sequences() -> None:
    plan = ValidatedFieldMetadataPlan()

    assert Solver.analyze(plan) == {"family": "soft", "levels": [0]}


@pytest.mark.parametrize(
    "field_name",
    [
        "validated_field_metadata_owners",
        "validated_field_metadata_order",
        "validated_field_metadata_durations",
        "validated_field_metadata_successors",
    ],
)
def test_field_backed_list_metadata_rejects_missing_solution_indexed_sequence(
    field_name: str,
) -> None:
    plan = ValidatedFieldMetadataPlan()
    delattr(plan, field_name)

    with pytest.raises(RuntimeError, match=rf"{field_name}.*missing"):
        Solver.analyze(plan)


@pytest.mark.parametrize(
    ("field_name", "value", "message"),
    [
        (
            "validated_field_metadata_owners",
            "not a sequence",
            "element_owner.*must be a sequence",
        ),
        (
            "validated_field_metadata_order",
            [0],
            "construction_element_order_key.*has no entry for element 1",
        ),
        (
            "validated_field_metadata_durations",
            [1, -1, 3],
            "precedence_duration.*must be a non-negative integer",
        ),
        (
            "validated_field_metadata_successors",
            [[], ["invalid"], []],
            "precedence_successors.*must be a sequence of non-negative integers",
        ),
    ],
)
def test_field_backed_list_metadata_rejects_malformed_solution_indexed_sequence(
    field_name: str,
    value: object,
    message: str,
) -> None:
    plan = ValidatedFieldMetadataPlan()
    setattr(plan, field_name, value)

    with pytest.raises(RuntimeError, match=message):
        Solver.analyze(plan)


field_metadata_callback_calls = 0


def lazy_field_metadata_callback(_solution: object, _element: int) -> object:
    global field_metadata_callback_calls
    field_metadata_callback_calls += 1
    raise AssertionError("field metadata callbacks must not run during state import")


@planning_entity
class LazyMetadataRoute:
    visits = planning_list_variable(
        element_collection="lazy_metadata_values",
        element_owner=lazy_field_metadata_callback,
        construction_element_order_key=lazy_field_metadata_callback,
        precedence_duration=lazy_field_metadata_callback,
        precedence_successors=lazy_field_metadata_callback,
    )

    def __init__(self) -> None:
        self.visits: list[int] = []


@planning_solution(score=SoftScore)
class LazyMetadataPlan:
    routes: list[LazyMetadataRoute]

    def __init__(self) -> None:
        self.routes = [LazyMetadataRoute()]
        self.lazy_metadata_values = [0]
        self.score = None


def test_callback_list_metadata_remains_lazy_during_state_import() -> None:
    global field_metadata_callback_calls
    field_metadata_callback_calls = 0

    assert Solver.analyze(LazyMetadataPlan()) == {"family": "soft", "levels": [0]}
    assert field_metadata_callback_calls == 0


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


def test_dynamic_list_construction_installs_field_backed_element_owner() -> None:
    plan = Solver.solve(
        FieldOwnedRoutePlan(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_cheapest_insertion",
                    "entity_class": "FieldOwnedRoute",
                    "variable_name": "visits",
                }
            ]
        },
    )

    assert [sorted(route.visits) for route in plan.field_owned_routes] == [
        [0, 2],
        [1, 3],
    ]


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


def precedence_owner(solution: object, operation_id: int) -> int:
    return int(solution.operation_owners[int(operation_id)])


PRECEDENCE_HOOK_CALLS = {"duration": 0, "successors": 0}


def precedence_duration(solution: object, operation_id: int) -> int:
    PRECEDENCE_HOOK_CALLS["duration"] += 1
    return int(solution.operation_durations[int(operation_id)])


def precedence_successors(solution: object, operation_id: int) -> list[int]:
    PRECEDENCE_HOOK_CALLS["successors"] += 1
    return list(solution.operation_successors[int(operation_id)])


@planning_entity
class PrecedenceMachine:
    operations = planning_list_variable(
        element_collection="operation_ids",
        element_owner=precedence_owner,
        precedence_duration=precedence_duration,
        precedence_successors=precedence_successors,
    )

    def __init__(self, operations: list[int] | None = None) -> None:
        self.operations = operations or []


@planning_entity
class MetadataPrecedenceMachine:
    operations = planning_list_variable(
        element_collection="field_operation_ids",
        element_owner="field_operation_owners",
        precedence_duration="field_operation_durations",
        precedence_successors="field_operation_successors",
    )

    def __init__(self, operations: list[int] | None = None) -> None:
        self.operations = operations or []


@constraint_provider
def precedence_schedule_constraints(factory: ConstraintFactory):
    return [
        factory.list_precedence_makespan(PrecedenceMachine, "operations").named(
            "precedence makespan"
        )
    ]


@constraint_provider
def metadata_precedence_schedule_constraints(factory: ConstraintFactory):
    return [
        factory.list_precedence_makespan(MetadataPrecedenceMachine, "operations").named(
            "metadata precedence makespan"
        )
    ]


@planning_solution(score=HardSoftScore, constraints=precedence_schedule_constraints)
class PrecedenceSchedulePlan:
    machine_sequences: list[PrecedenceMachine]

    def __init__(self, machine_sequences: list[list[int]]) -> None:
        self.machine_sequences = [
            PrecedenceMachine(operations) for operations in machine_sequences
        ]
        self.operation_ids = [0, 1, 2, 3]
        self.operation_owners = [0, 1, 1, 0]
        self.operation_durations = [3, 2, 4, 1]
        self.operation_successors = [[1], [], [3], []]
        self.score = None


@planning_solution(score=SoftScore, constraints=precedence_schedule_constraints)
class SoftPrecedenceSchedulePlan:
    machine_sequences: list[PrecedenceMachine]

    def __init__(self, machine_sequences: list[list[int]]) -> None:
        self.machine_sequences = [
            PrecedenceMachine(operations) for operations in machine_sequences
        ]
        self.operation_ids = [0, 1, 2, 3]
        self.operation_owners = [0, 1, 1, 0]
        self.operation_durations = [3, 2, 4, 1]
        self.operation_successors = [[1], [], [3], []]
        self.score = None


@planning_solution(
    score=HardSoftScore, constraints=metadata_precedence_schedule_constraints
)
class MetadataPrecedenceSchedulePlan:
    machine_sequences: list[MetadataPrecedenceMachine]

    def __init__(self, machine_sequences: list[list[int]]) -> None:
        self.machine_sequences = [
            MetadataPrecedenceMachine(operations) for operations in machine_sequences
        ]
        self.field_operation_ids = [0, 1, 2, 3]
        self.field_operation_owners = [0, 1, 1, 0]
        self.field_operation_durations = [3, 2, 4, 1]
        self.field_operation_successors = [[1], [], [3], []]
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


def test_dynamic_list_precedence_makespan_scores_valid_schedule() -> None:
    plan = PrecedenceSchedulePlan([[0, 3], [2, 1]])

    score = Solver.analyze(plan)

    assert score == {"family": "hard_soft", "levels": [0, -6]}
    assert plan.score == score


def test_dynamic_list_precedence_uses_solution_indexed_metadata() -> None:
    schema = build_schema(MetadataPrecedenceSchedulePlan([[0, 3], [2, 1]]))
    field = schema["entities"][0]["fields"][0]
    assert field["element_owner_field"] == "field_operation_owners"
    assert field["precedence_duration_field"] == "field_operation_durations"
    assert field["precedence_successors_field"] == "field_operation_successors"

    score = Solver.analyze(MetadataPrecedenceSchedulePlan([[0, 3], [2, 1]]))

    assert score == {"family": "hard_soft", "levels": [0, -6]}


def test_dynamic_list_precedence_makespan_scores_cycle_penalty() -> None:
    plan = PrecedenceSchedulePlan([[3, 0], [1, 2]])

    score = Solver.analyze(plan)

    assert score == {"family": "hard_soft", "levels": [-4, 0]}
    assert plan.score == score


def test_dynamic_list_precedence_makespan_preserves_soft_score_penalties() -> None:
    plan = SoftPrecedenceSchedulePlan([[3, 0], [1, 2]])

    score = Solver.analyze(plan)

    assert score == {"family": "soft", "levels": [-4]}
    assert plan.score == score


def test_dynamic_list_regret_construction_passes_precedence_hooks() -> None:
    PRECEDENCE_HOOK_CALLS["duration"] = 0
    PRECEDENCE_HOOK_CALLS["successors"] = 0

    plan = Solver.solve(
        PrecedenceSchedulePlan([[], []]),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_regret_insertion",
                    "entity_class": "PrecedenceMachine",
                    "variable_name": "operations",
                }
            ]
        },
    )
    duration_calls = PRECEDENCE_HOOK_CALLS["duration"]
    successor_calls = PRECEDENCE_HOOK_CALLS["successors"]

    assigned = sorted(
        operation
        for machine in plan.machine_sequences
        for operation in machine.operations
    )

    assert duration_calls > 0
    assert successor_calls > 0
    assert assigned == [0, 1, 2, 3]
    for machine_idx, machine in enumerate(plan.machine_sequences):
        assert all(
            plan.operation_owners[operation] == machine_idx
            for operation in machine.operations
        )
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"][0] == 0


def test_dynamic_list_precedence_makespan_scores_assignment_penalties() -> None:
    plan = PrecedenceSchedulePlan([[0, 1], [1, 2]])

    score = Solver.analyze(plan)

    assert score == {"family": "hard_soft", "levels": [-3, -10]}
    assert plan.score == score


def test_dynamic_list_precedence_makespan_updates_nonzero_owner_incrementally() -> None:
    plan = Solver.solve(
        PrecedenceSchedulePlan([[0, 3], [1, 2]]),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "list_permute_move_selector",
                        "selection_order": "original",
                        "entity_class": "PrecedenceMachine",
                        "variable_name": "operations",
                        "min_window_size": 2,
                        "max_window_size": 2,
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "score_tie_break": "first",
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    solved_score = plan.score
    fresh_score = Solver.analyze(plan)

    assert [machine.operations for machine in plan.machine_sequences] == [
        [0, 3],
        [2, 1],
    ]
    assert solved_score == {"family": "hard_soft", "levels": [0, -6]}
    assert fresh_score == solved_score


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


def list_local_search_config(move_selector: dict[str, object]) -> dict[str, object]:
    return {
        "random_seed": 17,
        "phases": [
            {
                "type": "local_search",
                "local_search_type": "acceptor_forager",
                "move_selector": move_selector,
                "acceptor": {"type": "hill_climbing"},
                "forager": {"type": "best_score"},
                "termination": {"step_count_limit": 2},
            }
        ],
    }


ORDINARY_LIST_MOVE_SELECTORS = [
    {
        "type": "list_change_move_selector",
        "entity_class": "Route",
        "variable_name": "visits",
    },
    {
        "type": "list_swap_move_selector",
        "entity_class": "Route",
        "variable_name": "visits",
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


@pytest.mark.parametrize("move_selector", ORDINARY_LIST_MOVE_SELECTORS)
def test_dynamic_list_local_search_supports_non_nearby_list_move_selectors_without_metrics(
    move_selector: dict[str, object],
) -> None:
    plan = Solver.solve(FourVisitRoutePlan(), list_local_search_config(move_selector))

    assert sorted(plan.routes[0].visits) == [0, 1, 2, 3]
    assert plan.score is not None


POSITION_METRIC_REQUIRED_MOVE_SELECTORS = [
    (
        {
            "type": "nearby_list_change_move_selector",
            "entity_class": "Route",
            "variable_name": "visits",
            "max_nearby": 4,
        },
        "cross-position list distance",
    ),
    (
        {
            "type": "nearby_list_swap_move_selector",
            "entity_class": "Route",
            "variable_name": "visits",
            "max_nearby": 4,
        },
        "cross-position list distance",
    ),
    (
        {
            "type": "k_opt_move_selector",
            "entity_class": "Route",
            "variable_name": "visits",
            "k": 2,
            "min_segment_len": 1,
            "max_nearby": 4,
        },
        "intra-position list distance",
    ),
]


@pytest.mark.parametrize(
    ("move_selector", "required_capability"), POSITION_METRIC_REQUIRED_MOVE_SELECTORS
)
def test_dynamic_nearby_list_selectors_require_declared_position_metrics(
    move_selector: dict[str, object], required_capability: str
) -> None:
    with pytest.raises(RuntimeError, match=required_capability):
        Solver.solve(FourVisitRoutePlan(), list_local_search_config(move_selector))


@pytest.mark.parametrize(
    ("move_selector", "required_capability"), POSITION_METRIC_REQUIRED_MOVE_SELECTORS
)
def test_retained_nearby_list_selector_without_position_metric_fails_once(
    move_selector: dict[str, object], required_capability: str
) -> None:
    manager = SolverManager(list_local_search_config(move_selector))
    handle = manager.solve(FourVisitRoutePlan())
    status = manager.wait(handle.job_id)
    events = manager.events(handle.job_id)
    failed = [event for event in events if event["event_type"] == "FAILED"]
    terminal = [
        event
        for event in events
        if event["event_type"] in {"COMPLETED", "CANCELLED", "FAILED"}
    ]

    assert status["lifecycle_state"] == "FAILED"
    assert len(failed) == 1
    assert [event["event_type"] for event in terminal] == ["FAILED"]
    assert required_capability in failed[0]["error"]
    manager.delete(handle.job_id)


@planning_entity
class FieldPositionMetricRoute:
    visits = planning_list_variable(
        element_collection="field_position_metric_visit_values",
        cross_position_distance=RowField("position_distance_matrix"),
        intra_position_distance=RowField("position_distance_matrix"),
    )

    def __init__(self, visits: list[int] | None = None) -> None:
        self.position_distance_matrix = [
            [0, 1, 2, 3],
            [1, 0, 1, 2],
            [2, 1, 0, 1],
            [3, 2, 1, 0],
        ]
        self.visits = list(visits or [])


@constraint_provider
def prefer_sorted_field_position_metric_route(factory: ConstraintFactory):
    return [
        factory.for_each(FieldPositionMetricRoute)
        .filter(lambda route: route.visits != sorted(route.visits))
        .penalize(SoftScore.of(1))
        .named("prefer sorted field-backed position metric route")
    ]


@planning_solution(
    score=SoftScore,
    constraints=prefer_sorted_field_position_metric_route,
)
class FieldPositionMetricPlan:
    field_position_metric_routes: list[FieldPositionMetricRoute]

    def __init__(self) -> None:
        self.field_position_metric_routes = [FieldPositionMetricRoute([3, 2, 1, 0])]
        self.field_position_metric_visit_values = [0, 1, 2, 3]
        self.score = None


def test_field_backed_position_metrics_are_collected_without_callbacks() -> None:
    schema = build_schema(FieldPositionMetricPlan())
    metadata = schema["entities"][0]["fields"][0]["list_metadata"]

    assert metadata["cross_position_distance"] == {
        "kind": "row",
        "field": "position_distance_matrix",
    }
    assert metadata["intra_position_distance"] == {
        "kind": "row",
        "field": "position_distance_matrix",
    }


@pytest.mark.parametrize(
    "move_selector",
    [
        {
            "type": "nearby_list_change_move_selector",
            "entity_class": "FieldPositionMetricRoute",
            "variable_name": "visits",
            "max_nearby": 4,
        },
        {
            "type": "nearby_list_swap_move_selector",
            "entity_class": "FieldPositionMetricRoute",
            "variable_name": "visits",
            "max_nearby": 4,
        },
        {
            "type": "k_opt_move_selector",
            "entity_class": "FieldPositionMetricRoute",
            "variable_name": "visits",
            "k": 2,
            "min_segment_len": 1,
            "max_nearby": 4,
        },
    ],
)
def test_field_backed_position_metrics_preserve_direct_retained_search_parity(
    move_selector: dict[str, object],
) -> None:
    direct = Solver.solve(
        FieldPositionMetricPlan(), list_local_search_config(move_selector)
    )
    manager = SolverManager(list_local_search_config(move_selector))
    handle = manager.solve(FieldPositionMetricPlan())
    status = manager.wait(handle.job_id)
    snapshot = manager.snapshot(handle.job_id)
    events = manager.events(handle.job_id)
    terminal = [
        event
        for event in events
        if event["event_type"] in {"COMPLETED", "CANCELLED", "FAILED"}
    ]

    assert sorted(direct.field_position_metric_routes[0].visits) == [0, 1, 2, 3]
    assert direct.score == Solver.analyze(direct)
    assert status["lifecycle_state"] == "COMPLETED"
    assert status["best_score"] == direct.score == snapshot.score
    assert [route.visits for route in snapshot.field_position_metric_routes] == [
        route.visits for route in direct.field_position_metric_routes
    ]
    assert [event["event_type"] for event in terminal] == ["COMPLETED"]
    manager.delete(handle.job_id)


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
                                "selection_order": "original",
                                "entity_class": "Route",
                                "variable_name": "visits",
                            },
                            {
                                "type": "list_reverse_move_selector",
                                "selection_order": "original",
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
                                "selection_order": "original",
                                "entity_class": "Route",
                                "variable_name": "visits",
                            },
                            {
                                "type": "change_move_selector",
                                "selection_order": "original",
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
                        "selection_order": "sequential",
                        "selectors": [
                            {
                                "type": "list_reverse_move_selector",
                                "selection_order": "original",
                                "entity_class": "Route",
                                "variable_name": "visits",
                            },
                            {
                                "type": "change_move_selector",
                                "selection_order": "original",
                                "entity_class": "MixedTask",
                                "variable_name": "worker",
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "score_tie_break": "first",
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


@planning_entity
class CvrpRoute:
    visits = planning_list_variable(
        element_collection="visit_values",
        route=ListRouteHooks(
            depot=EntityCallback(route_depot),
            distance=EntityCallback(route_distance),
            feasible=EntityCallback(route_feasible),
        ),
        savings=ListSavingsHooks(
            depot=EntityCallback(route_depot),
            metric_class=EntityCallback(route_depot),
            distance=EntityCallback(route_distance),
            feasible=EntityCallback(route_feasible),
        ),
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


def cvrp_clarke_wright_k_opt_config(
    *, candidate_trace_max_entries: int | None = None
) -> dict[str, object]:
    config: dict[str, object] = {
        "random_seed": 17,
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
        ],
    }
    if candidate_trace_max_entries is not None:
        config["candidate_trace"] = {"max_entries": candidate_trace_max_entries}
    return config


def test_dynamic_cvrp_route_hooks_bind_list_clarke_wright_and_k_opt() -> None:
    plan = Solver.solve(CvrpPlan(), cvrp_clarke_wright_k_opt_config())

    assigned = sorted(visit for route in plan.cvrp_routes for visit in route.visits)
    assert assigned == [0, 1, 2]
    assert all(len(route.visits) <= route.capacity for route in plan.cvrp_routes)


def test_retained_cvrp_uses_configured_clarke_wright_not_cheapest_insertion() -> None:
    """The dynamic bridge must execute the configured core CW phase itself.

    This is intentionally a retained candidate-trace assertion rather than a
    final-route assertion alone: a Python-owned pre-assignment loop or a
    construction fallback could otherwise return a valid route while silently
    bypassing the configured Clarke-Wright work.
    """
    direct = Solver.solve(CvrpPlan(), cvrp_clarke_wright_k_opt_config())
    manager = SolverManager(
        cvrp_clarke_wright_k_opt_config(candidate_trace_max_entries=512)
    )
    handle = manager.solve(CvrpPlan())
    status = manager.wait(handle.job_id)
    trace = manager.telemetry_detail(handle.job_id)["candidate_trace"]
    snapshot = manager.snapshot(handle.job_id)

    assert status["lifecycle_state"] == "COMPLETED"
    assert status["best_score"] == direct.score == snapshot.score
    assert [route.visits for route in snapshot.cvrp_routes] == [
        route.visits for route in direct.cvrp_routes
    ]
    assert isinstance(trace, dict)
    sources = {pull["source"] for pull in trace["pulls"]}
    assert sources & {
        "list_clarke_wright_savings",
        "list_clarke_wright_merge",
        "list_clarke_wright_completion_insertion",
    }
    assert "list_cheapest_insertion_trial" not in sources
    manager.delete(handle.job_id)


def test_route_callback_metadata_is_classified_by_signature() -> None:
    schema = build_schema(CvrpPlan())
    field = schema["entities"][0]["fields"][0]
    route = field["list_metadata"]["route"]

    assert route["depot"]["kind"] == "entity"
    assert callable(route["depot"]["callback"])
    assert route["distance"]["kind"] == "entity"
    assert callable(route["distance"]["callback"])
    assert route["feasible"]["kind"] == "entity"
    assert callable(route["feasible"]["callback"])


def cross_position_metric(
    from_route: object,
    from_position: int,
    to_route: object,
    to_position: int,
) -> int:
    del from_route, from_position, to_route, to_position
    return 0


def intra_position_metric(
    solution: object,
    entity_index: int,
    from_position: int,
    to_position: int,
) -> int:
    del solution, entity_index, from_position, to_position
    return 0


@planning_entity
class PositionMetricRoute:
    visits = planning_list_variable(
        element_collection="position_metric_visit_values",
        cross_position_distance=EntityCallback(cross_position_metric),
        intra_position_distance=SolutionCallback(intra_position_metric),
    )


def test_position_metric_callbacks_are_classified_by_signature() -> None:
    field = PositionMetricRoute.__solverforge_entity__["fields"][0]
    metadata = field["list_metadata"]
    assert metadata["cross_position_distance"]["kind"] == "entity"
    assert callable(metadata["cross_position_distance"]["callback"])
    assert metadata["intra_position_distance"]["kind"] == "solution"
    assert callable(metadata["intra_position_distance"]["callback"])


@planning_entity
class RowMetadataRoute:
    visits = planning_list_variable(
        element_collection="row_metadata_visit_values",
        route=ROW_ROUTE_HOOKS,
        savings=ROW_SAVINGS_HOOKS,
        cross_position_distance=RowField("distance_matrix"),
        intra_position_distance=RowField("distance_matrix"),
    )

    def __init__(self) -> None:
        self.depot = 3
        self.metric_class = 0
        self.capacity = 2
        self.demands = [1, 1, 1]
        self.distance_matrix = [
            [0, 8, 2, 2],
            [8, 0, 2, 2],
            [2, 2, 0, 2],
            [2, 2, 2, 0],
        ]
        self.visits: list[int] = []


@constraint_provider
def row_metadata_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(RowMetadataRoute)
        .filter(lambda route: bool(route.visits))
        .reward(HardSoftScore.of_soft(1))
        .named("non-empty row metadata route")
    ]


@planning_solution(score=HardSoftScore, constraints=row_metadata_constraints)
class RowMetadataPlan:
    row_metadata_routes: list[RowMetadataRoute]

    def __init__(self) -> None:
        self.row_metadata_routes = [RowMetadataRoute(), RowMetadataRoute()]
        self.row_metadata_visit_values = [0, 1, 2]
        self.score = None


@planning_entity
class RowCapacityBackedRoute:
    visits = planning_list_variable(
        element_collection="row_capacity_visit_values",
        route=ROW_ROUTE_HOOKS,
        savings=ROW_SAVINGS_HOOKS,
    )

    def __init__(self) -> None:
        self.depot = 2
        self.metric_class = 0
        self.capacity = 1
        self.demands = [1, 1]
        self.distance_matrix = [
            [0, 1, 1],
            [1, 0, 1],
            [1, 1, 0],
        ]
        self.visits: list[int] = []


@constraint_provider
def row_capacity_constraints(factory: ConstraintFactory):
    return [
        factory.for_each_unassigned_element(RowCapacityBackedRoute, "visits")
        .penalize(HardSoftScore.ONE_HARD)
        .named("row-capacity all visits assigned")
    ]


@planning_solution(score=HardSoftScore, constraints=row_capacity_constraints)
class RowCapacityBackedPlan:
    row_capacity_routes: list[RowCapacityBackedRoute]

    def __init__(self) -> None:
        self.row_capacity_routes = [RowCapacityBackedRoute(), RowCapacityBackedRoute()]
        self.row_capacity_visit_values = [0, 1]
        self.capacity = 2
        self.demands = [1, 1]
        self.distance_matrix = [
            [0, 1, 1],
            [1, 0, 1],
            [1, 1, 0],
        ]
        self.score = None


def test_row_metadata_route_hooks_use_row_sources() -> None:
    plan = Solver.solve(
        RowMetadataPlan(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_clarke_wright",
                    "entity_class": "RowMetadataRoute",
                    "variable_name": "visits",
                },
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_k_opt",
                    "entity_class": "RowMetadataRoute",
                    "variable_name": "visits",
                    "k": 2,
                },
            ]
        },
    )

    assigned = sorted(
        visit for route in plan.row_metadata_routes for visit in route.visits
    )
    assert assigned == [0, 1, 2]
    assert all(
        len(route.visits) <= route.capacity for route in plan.row_metadata_routes
    )


def test_row_metadata_route_hooks_preserve_row_fields_that_share_solution_names() -> (
    None
):
    plan = Solver.solve(
        RowCapacityBackedPlan(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_clarke_wright",
                    "entity_class": "RowCapacityBackedRoute",
                    "variable_name": "visits",
                }
            ]
        },
    )

    assigned = sorted(
        visit for route in plan.row_capacity_routes for visit in route.visits
    )
    assert assigned == [0, 1]
    assert all(len(route.visits) <= 1 for route in plan.row_capacity_routes)
    assert plan.score == {"family": "hard_soft", "levels": [0, 0]}


@planning_entity
class PropertyCapacityBackedRoute:
    visits = planning_list_variable(
        element_collection="property_capacity_visit_values",
        route=ROW_ROUTE_HOOKS,
        savings=ROW_SAVINGS_HOOKS,
    )

    def __init__(self) -> None:
        self.depot = 2
        self.metric_class = 0
        self._capacity = 1
        self._demands = [1, 1]
        self._distance_matrix = [
            [0, 1, 1],
            [1, 0, 1],
            [1, 1, 0],
        ]
        self.visits: list[int] = []

    @property
    def capacity(self) -> int:
        return self._capacity

    @property
    def demands(self) -> list[int]:
        return self._demands

    @property
    def distance_matrix(self) -> list[list[int]]:
        return self._distance_matrix


@constraint_provider
def property_capacity_constraints(factory: ConstraintFactory):
    return [
        factory.for_each_unassigned_element(PropertyCapacityBackedRoute, "visits")
        .penalize(HardSoftScore.ONE_HARD)
        .named("property-capacity all visits assigned")
    ]


@planning_solution(score=HardSoftScore, constraints=property_capacity_constraints)
class PropertyCapacityBackedPlan:
    property_capacity_routes: list[PropertyCapacityBackedRoute]

    def __init__(self) -> None:
        self.property_capacity_routes = [
            PropertyCapacityBackedRoute(),
            PropertyCapacityBackedRoute(),
        ]
        self.property_capacity_visit_values = [0, 1]
        self.capacity = 2
        self.demands = [1, 1]
        self.distance_matrix = [
            [0, 1, 1],
            [1, 0, 1],
            [1, 1, 0],
        ]
        self.score = None


def test_row_metadata_route_hooks_import_read_only_row_fields_that_share_solution_names() -> (
    None
):
    plan = Solver.solve(
        PropertyCapacityBackedPlan(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_clarke_wright",
                    "entity_class": "PropertyCapacityBackedRoute",
                    "variable_name": "visits",
                }
            ]
        },
    )

    assigned = sorted(
        visit for route in plan.property_capacity_routes for visit in route.visits
    )
    assert assigned == [0, 1]
    assert all(len(route.visits) <= 1 for route in plan.property_capacity_routes)
    assert plan.score == {"family": "hard_soft", "levels": [0, 0]}


def shared_solution_route_distance(
    solution: object,
    _route_index: int,
    from_element: int,
    to_element: int,
) -> int:
    return int(solution.distance_matrix[from_element][to_element])


def shared_solution_route_feasible(
    solution: object,
    _route_index: int,
    visits: list[int],
) -> bool:
    return sum(int(solution.demands[visit]) for visit in visits) <= 2


@planning_entity
class SharedMetadataRoute:
    visits = planning_list_variable(
        element_collection="shared_row_metadata_visit_values",
        route=ListRouteHooks(
            depot=RowField("depot"),
            distance=SolutionCallback(shared_solution_route_distance),
            feasible=SolutionCallback(shared_solution_route_feasible),
        ),
        savings=ListSavingsHooks(
            depot=RowField("depot"),
            metric_class=RowField("metric_class"),
            distance=SolutionCallback(shared_solution_route_distance),
            feasible=SolutionCallback(shared_solution_route_feasible),
        ),
    )

    def __init__(self) -> None:
        self.depot = 3
        self.metric_class = 0
        self.capacity = 2
        self.visits: list[int] = []

    @property
    def demands(self) -> list[int]:
        raise AssertionError("shared demand field should not be imported per row")

    @property
    def distance_matrix(self) -> list[list[int]]:
        raise AssertionError("shared distance matrix should not be imported per row")


@constraint_provider
def shared_row_metadata_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(SharedMetadataRoute)
        .filter(lambda route: bool(route.visits))
        .reward(HardSoftScore.of_soft(1))
        .named("non-empty shared-row metadata route")
    ]


@planning_solution(score=HardSoftScore, constraints=shared_row_metadata_constraints)
class SharedMetadataPlan:
    shared_row_metadata_routes: list[SharedMetadataRoute]

    def __init__(self) -> None:
        self.shared_row_metadata_routes = [
            SharedMetadataRoute(),
            SharedMetadataRoute(),
        ]
        self.shared_row_metadata_visit_values = [0, 1, 2]
        self.demands = [1, 1, 1]
        self.distance_matrix = [
            [0, 8, 2, 2],
            [8, 0, 2, 2],
            [2, 2, 0, 2],
            [2, 2, 2, 0],
        ]
        self.score = None


@planning_entity
class ImplicitSharedMetadataRoute:
    visits = planning_list_variable(
        element_collection="implicit_shared_metadata_visit_values",
        route=ROW_ROUTE_HOOKS,
        savings=ROW_SAVINGS_HOOKS,
    )

    def __init__(self) -> None:
        self.depot = 3
        self.metric_class = 0
        self.capacity = 2
        self.visits: list[int] = []


@planning_solution(score=HardSoftScore)
class ImplicitSharedMetadataPlan:
    implicit_shared_metadata_routes: list[ImplicitSharedMetadataRoute]

    def __init__(self) -> None:
        self.implicit_shared_metadata_routes = [ImplicitSharedMetadataRoute()]
        self.implicit_shared_metadata_visit_values = [0, 1, 2]
        self.demands = [1, 1, 1]
        self.distance_matrix = [
            [0, 8, 2, 2],
            [8, 0, 2, 2],
            [2, 2, 0, 2],
            [2, 2, 2, 0],
        ]
        self.score = None


def test_solution_scoped_route_hooks_use_solution_level_shared_fields() -> None:
    plan = Solver.solve(
        SharedMetadataPlan(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_clarke_wright",
                    "entity_class": "SharedMetadataRoute",
                    "variable_name": "visits",
                },
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_k_opt",
                    "entity_class": "SharedMetadataRoute",
                    "variable_name": "visits",
                    "k": 2,
                },
            ]
        },
    )

    assigned = sorted(
        visit for route in plan.shared_row_metadata_routes for visit in route.visits
    )
    assert assigned == [0, 1, 2]
    assert all(
        len(route.visits) <= route.capacity for route in plan.shared_row_metadata_routes
    )


def test_row_scoped_route_metadata_never_falls_back_to_solution_fields() -> None:
    with pytest.raises(RuntimeError, match="row-scoped metadata never falls back"):
        Solver.solve(ImplicitSharedMetadataPlan())


@planning_entity
class InvalidPositionMetricRoute:
    visits = planning_list_variable(
        element_collection="invalid_position_metric_visit_values",
        cross_position_distance=RowField("matrix"),
    )

    def __init__(self) -> None:
        self.matrix = [[0]]
        self.visits: list[int] = []


@planning_solution(score=HardSoftScore)
class InvalidPositionMetricPlan:
    invalid_position_metric_routes: list[InvalidPositionMetricRoute]

    def __init__(self) -> None:
        self.invalid_position_metric_routes = [InvalidPositionMetricRoute()]
        self.invalid_position_metric_visit_values = [0, 1]
        self.score = None


def test_position_metric_matrix_is_validated_against_declared_element_values() -> None:
    with pytest.raises(
        RuntimeError,
        match="cross_position_distance.*no integer distance from 0 to 1",
    ):
        Solver.solve(InvalidPositionMetricPlan())


def shared_constraint_route_load(route: object) -> int:
    return sum(int(route.demands[visit]) for visit in route.visits)


def shared_constraint_route_cost(route: object) -> int:
    if not route.visits:
        return 0
    total = 0
    previous = int(route.depot)
    for visit in route.visits:
        total += int(route.distance_matrix[previous][visit])
        previous = int(visit)
    return total + int(route.distance_matrix[previous][int(route.depot)])


@planning_entity
class SharedConstraintRoute:
    visits = planning_list_variable(
        element_collection="shared_constraint_visit_values",
        route=ROW_ROUTE_HOOKS,
        savings=ROW_SAVINGS_HOOKS,
    )

    def __init__(self, demands: list[int], distance_matrix: list[list[int]]) -> None:
        self.depot = 3
        self.metric_class = 0
        self.capacity = 2
        self._demands = demands
        self._distance_matrix = distance_matrix
        self.visits: list[int] = []

    @property
    def demands(self) -> list[int]:
        return self._demands

    @property
    def distance_matrix(self) -> list[list[int]]:
        return self._distance_matrix


@constraint_provider
def shared_constraint_constraints(factory: ConstraintFactory):
    return [
        factory.for_each_unassigned_element(SharedConstraintRoute, "visits")
        .penalize(HardSoftScore.of_hard(1))
        .named("shared all visits assigned"),
        factory.for_each(SharedConstraintRoute)
        .filter(lambda route: shared_constraint_route_load(route) > int(route.capacity))
        .penalize(
            lambda route: HardSoftScore.of_hard(
                shared_constraint_route_load(route) - int(route.capacity)
            )
        )
        .named("shared route capacity"),
        factory.for_each(SharedConstraintRoute)
        .penalize(
            lambda route: HardSoftScore.of_soft(shared_constraint_route_cost(route))
        )
        .named("shared route distance"),
    ]


@planning_solution(score=HardSoftScore, constraints=shared_constraint_constraints)
class SharedConstraintPlan:
    shared_constraint_routes: list[SharedConstraintRoute]

    def __init__(self) -> None:
        self.shared_constraint_visit_values = [0, 1, 2]
        self.demands = [1, 1, 1]
        self.distance_matrix = [
            [0, 8, 2, 2],
            [8, 0, 2, 2],
            [2, 2, 0, 2],
            [2, 2, 2, 0],
        ]
        self.shared_constraint_routes = [
            SharedConstraintRoute(self.demands, self.distance_matrix),
            SharedConstraintRoute(self.demands, self.distance_matrix),
        ]
        self.score = None


def test_constraints_use_live_rows_with_solution_level_shared_fields() -> None:
    plan = Solver.solve(
        SharedConstraintPlan(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_clarke_wright",
                    "entity_class": "SharedConstraintRoute",
                    "variable_name": "visits",
                },
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_k_opt",
                    "entity_class": "SharedConstraintRoute",
                    "variable_name": "visits",
                    "k": 2,
                },
            ]
        },
    )

    assigned = sorted(
        visit for route in plan.shared_constraint_routes for visit in route.visits
    )
    assert assigned == [0, 1, 2]
    assert all(
        shared_constraint_route_load(route) <= route.capacity
        for route in plan.shared_constraint_routes
    )


callback_route_ids: list[int] = []
callback_route_visit_snapshots: list[tuple[int, ...]] = []


def live_route_depot(route: object) -> int:
    callback_route_ids.append(id(route))
    callback_route_visit_snapshots.append(tuple(route.visits))
    return int(route.depot)


def live_route_distance(route: object, from_element: int, to_element: int) -> int:
    callback_route_ids.append(id(route))
    callback_route_visit_snapshots.append(tuple(route.visits))
    return int(route.distance_matrix[from_element][to_element])


def live_route_feasible(route: object, visits: list[int]) -> bool:
    callback_route_ids.append(id(route))
    callback_route_visit_snapshots.append(tuple(route.visits))
    return sum(route.demands[visit] for visit in visits) <= route.capacity


@planning_entity
class LiveCallbackRoute:
    visits = planning_list_variable(
        element_collection="live_callback_visit_values",
        route=ListRouteHooks(
            depot=EntityCallback(live_route_depot),
            distance=EntityCallback(live_route_distance),
            feasible=EntityCallback(live_route_feasible),
        ),
        savings=ListSavingsHooks(
            depot=EntityCallback(live_route_depot),
            metric_class=EntityCallback(live_route_depot),
            distance=EntityCallback(live_route_distance),
            feasible=EntityCallback(live_route_feasible),
        ),
    )

    def __init__(self) -> None:
        self.depot = 3
        self.capacity = 2
        self.demands = [1, 1, 1]
        self.distance_matrix = [
            [0, 8, 2, 2],
            [8, 0, 2, 2],
            [2, 2, 0, 2],
            [2, 2, 2, 0],
        ]
        self.visits: list[int] = []


@constraint_provider
def live_callback_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(LiveCallbackRoute)
        .filter(lambda route: bool(route.visits))
        .reward(HardSoftScore.of_soft(1))
        .named("non-empty live callback route")
    ]


@planning_solution(score=HardSoftScore, constraints=live_callback_constraints)
class LiveCallbackPlan:
    live_callback_routes: list[LiveCallbackRoute]

    def __init__(self) -> None:
        self.live_callback_routes = [LiveCallbackRoute(), LiveCallbackRoute()]
        self.live_callback_visit_values = [0, 1, 2]
        self.score = None


def test_route_row_callbacks_use_synced_live_python_rows() -> None:
    callback_route_ids.clear()
    callback_route_visit_snapshots.clear()
    plan = LiveCallbackPlan()
    original_route_ids = {id(route) for route in plan.live_callback_routes}

    Solver.solve(
        plan,
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_clarke_wright",
                    "entity_class": "LiveCallbackRoute",
                    "variable_name": "visits",
                },
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "list_k_opt",
                    "entity_class": "LiveCallbackRoute",
                    "variable_name": "visits",
                    "k": 2,
                },
            ]
        },
    )

    assert callback_route_ids
    assert set(callback_route_ids).issubset(original_route_ids)
    assert any(snapshot for snapshot in callback_route_visit_snapshots)
