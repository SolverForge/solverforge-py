from typing import cast

import pytest

from solverforge import _native
from solverforge import (
    CapacityRouteFeasibility,
    EntityCallback,
    ListRouteHooks,
    ListSavingsHooks,
    RowField,
    SolutionCallback,
    SolutionField,
    planning_entity,
    planning_solution,
    planning_variable,
    planning_list_variable,
    problem_fact,
    scalar_assignment_group,
)
from solverforge.model import _compiled_schema_for_solution, build_schema


@planning_entity
class Task:
    worker = planning_variable(value_range_provider="workers", allows_unassigned=True)

    def __init__(self) -> None:
        self.worker = None


@planning_solution()
class Plan:
    tasks: list[Task]

    def __init__(self) -> None:
        self.tasks = [Task()]
        self.workers = [0, 1]
        self.score = None


def test_schema_collects_entity_and_variable_metadata() -> None:
    schema = build_schema(Plan())
    assert schema["solution_type"] == "Plan"
    assert schema["entities"][0]["type_name"] == "Task"
    assert schema["entities"][0]["fields"][0]["name"] == "worker"


def test_schema_dict_compiles_for_runtime_cache() -> None:
    compiled = _native.compile_schema(build_schema(Plan()))

    assert compiled.solution_type == "Plan"
    assert compiled.score_family == "hard_soft"


@pytest.mark.parametrize(
    ("callback_key", "field_key"),
    [
        ("nearby_value_candidates", "nearby_value_candidates_field"),
        ("nearby_entity_candidates", "nearby_entity_candidates_field"),
        ("nearby_value_distance_meter", "nearby_value_distance_field"),
        ("nearby_entity_distance_meter", "nearby_entity_distance_field"),
    ],
)
def test_compiled_schema_rejects_dual_nearby_metadata_sources(
    callback_key: str, field_key: str
) -> None:
    schema = build_schema(Plan())
    entity = dict(schema["entities"][0])
    field = dict(entity["fields"][0])
    entity["fields"] = [field]
    schema["entities"] = [entity]
    field[callback_key] = lambda *_args: None
    field[field_key] = "nearby_metadata"

    with pytest.raises(RuntimeError, match=f"{callback_key}.*{field_key}"):
        _native.compile_schema(schema)


def test_compiled_schema_rejects_row_route_feasibility_source() -> None:
    schema = build_schema(Plan())
    entity = dict(schema["entities"][0])
    field = dict(entity["fields"][0])
    entity["fields"] = [field]
    schema["entities"] = [entity]
    field["kind"] = "planning_list_variable"
    field["element_collection"] = "workers"
    field["list_metadata"] = {
        "route": {
            "depot": {"kind": "row", "field": "depot"},
            "distance": {"kind": "row", "field": "matrix"},
            "feasible": {"kind": "row", "field": "feasible"},
        },
        "savings": None,
        "cross_position_distance": None,
        "intra_position_distance": None,
    }

    with pytest.raises(
        RuntimeError,
        match="list_metadata.route.feasible.*does not support a field source",
    ):
        _native.compile_schema(schema)


def test_compiled_schema_rejects_list_metadata_on_scalar_variable() -> None:
    schema = build_schema(Plan())
    entity = dict(schema["entities"][0])
    field = dict(entity["fields"][0])
    entity["fields"] = [field]
    schema["entities"] = [entity]
    field["list_metadata"] = {
        "route": None,
        "savings": None,
        "cross_position_distance": None,
        "intra_position_distance": None,
    }

    with pytest.raises(RuntimeError, match="only valid on planning_list_variable"):
        _native.compile_schema(schema)


def test_compiled_schema_rejects_empty_raw_route_field_name() -> None:
    schema = build_schema(Plan())
    entity = dict(schema["entities"][0])
    field = dict(entity["fields"][0])
    entity["fields"] = [field]
    schema["entities"] = [entity]
    field["kind"] = "planning_list_variable"
    field["element_collection"] = "workers"
    field["list_metadata"] = {
        "route": {
            "depot": {"kind": "row", "field": "depot"},
            "distance": {"kind": "row", "field": ""},
            "feasible": {
                "kind": "capacity",
                "capacity": {"kind": "row", "field": "capacity"},
                "demand": {"kind": "row", "field": "demands"},
            },
        },
        "savings": None,
        "cross_position_distance": None,
        "intra_position_distance": None,
    }

    with pytest.raises(RuntimeError, match="field.*non-empty string"):
        _native.compile_schema(schema)


def _raw_canonical_list_schema() -> tuple[dict[str, object], dict[str, object]]:
    def entity_depot(_route: object) -> int:
        return 0

    def entity_distance(_route: object, _from_value: int, _to_value: int) -> int:
        return 0

    def entity_feasible(_route: object, _values: list[int]) -> bool:
        return True

    def solution_depot(_solution: object, _entity_index: int) -> int:
        return 0

    def solution_distance(
        _solution: object,
        _entity_index: int,
        _from_value: int,
        _to_value: int,
    ) -> int:
        return 0

    def solution_feasible(
        _solution: object, _entity_index: int, _values: list[int]
    ) -> bool:
        return True

    def cross_distance(
        _from_route: object,
        _from_position: int,
        _to_route: object,
        _to_position: int,
    ) -> int:
        return 0

    def intra_distance(
        _solution: object,
        _entity_index: int,
        _from_position: int,
        _to_position: int,
    ) -> int:
        return 0

    schema = build_schema(Plan())
    entity = dict(schema["entities"][0])
    field = dict(entity["fields"][0])
    entity["fields"] = [field]
    schema["entities"] = [entity]
    field["kind"] = "planning_list_variable"
    field["element_collection"] = "workers"
    field["list_metadata"] = {
        "route": {
            "depot": {"kind": "entity", "callback": entity_depot},
            "distance": {"kind": "entity", "callback": entity_distance},
            "feasible": {"kind": "entity", "callback": entity_feasible},
        },
        "savings": {
            "depot": {"kind": "solution", "callback": solution_depot},
            "metric_class": {"kind": "entity", "callback": entity_depot},
            "distance": {"kind": "solution", "callback": solution_distance},
            "feasible": {"kind": "solution", "callback": solution_feasible},
        },
        "cross_position_distance": {"kind": "entity", "callback": cross_distance},
        "intra_position_distance": {"kind": "solution", "callback": intra_distance},
    }
    return schema, field


@pytest.mark.parametrize(
    ("path", "scope", "arity"),
    [
        ("route.depot", "entity", 1),
        ("route.depot", "solution", 2),
        ("route.distance", "entity", 3),
        ("route.distance", "solution", 4),
        ("route.feasible", "entity", 2),
        ("route.feasible", "solution", 3),
        ("savings.depot", "entity", 1),
        ("savings.depot", "solution", 2),
        ("savings.metric_class", "entity", 1),
        ("savings.metric_class", "solution", 2),
        ("savings.distance", "entity", 3),
        ("savings.distance", "solution", 4),
        ("savings.feasible", "entity", 2),
        ("savings.feasible", "solution", 3),
        ("cross_position_distance", "entity", 4),
        ("cross_position_distance", "solution", 5),
        ("intra_position_distance", "entity", 3),
        ("intra_position_distance", "solution", 4),
    ],
)
def test_raw_canonical_list_callbacks_require_their_hook_arity(
    path: str, scope: str, arity: int
) -> None:
    schema, field = _raw_canonical_list_schema()
    target = cast(dict[str, object], field["list_metadata"])
    path_parts = path.split(".")
    for part in path_parts[:-1]:
        target = cast(dict[str, object], target[part])
    target[path_parts[-1]] = {"kind": scope, "callback": lambda: 0}

    context = f"list_metadata.{path}"
    with pytest.raises(
        RuntimeError, match=rf"{context}.*{scope} callback must accept {arity}"
    ):
        _native.validate_schema(schema)


def test_nested_list_metadata_serializes_explicit_source_scopes() -> None:
    def entity_feasible(_route: object, _values: list[int]) -> bool:
        return True

    def cross_distance(
        _from_route: object,
        _from_position: int,
        _to_route: object,
        _to_position: int,
    ) -> int:
        return 0

    def intra_distance(
        _solution: object,
        _entity_index: int,
        _from_position: int,
        _to_position: int,
    ) -> int:
        return 0

    @planning_entity
    class CanonicalRoute:
        visits = planning_list_variable(
            element_collection="values",
            route=ListRouteHooks(
                depot=RowField("depot"),
                distance=SolutionField("distance_matrix"),
                feasible=EntityCallback(entity_feasible),
            ),
            savings=ListSavingsHooks(
                depot=SolutionField("depot"),
                metric_class=RowField("metric_class"),
                distance=SolutionField("distance_matrix"),
                feasible=CapacityRouteFeasibility(
                    capacity=RowField("capacity"),
                    demand=SolutionField("demands"),
                ),
            ),
            cross_position_distance=EntityCallback(cross_distance),
            intra_position_distance=SolutionCallback(intra_distance),
        )

    metadata = CanonicalRoute.__solverforge_entity__["fields"][0]["list_metadata"]  # type: ignore[attr-defined]
    assert metadata == {
        "route": {
            "depot": {"kind": "row", "field": "depot"},
            "distance": {"kind": "solution_field", "field": "distance_matrix"},
            "feasible": {"kind": "entity", "callback": entity_feasible},
        },
        "savings": {
            "depot": {"kind": "solution_field", "field": "depot"},
            "metric_class": {"kind": "row", "field": "metric_class"},
            "distance": {"kind": "solution_field", "field": "distance_matrix"},
            "feasible": {
                "kind": "capacity",
                "capacity": {"kind": "row", "field": "capacity"},
                "demand": {"kind": "solution_field", "field": "demands"},
            },
        },
        "cross_position_distance": {"kind": "entity", "callback": cross_distance},
        "intra_position_distance": {"kind": "solution", "callback": intra_distance},
    }


def test_explicit_route_and_savings_bundles_do_not_couple_equal_callbacks() -> None:
    def strict_feasible(
        _solution: object, _entity_index: int, _values: list[int]
    ) -> bool:
        return True

    @planning_entity
    class StrictRoute:
        visits = planning_list_variable(
            element_collection="values",
            route=ListRouteHooks(
                depot=RowField("depot"),
                distance=RowField("distance_matrix"),
                feasible=SolutionCallback(strict_feasible),
            ),
            savings=ListSavingsHooks(
                depot=RowField("depot"),
                metric_class=RowField("metric_class"),
                distance=RowField("distance_matrix"),
                feasible=SolutionCallback(strict_feasible),
            ),
        )

    metadata = StrictRoute.__solverforge_entity__["fields"][0]["list_metadata"]  # type: ignore[attr-defined]
    route = metadata["route"]
    savings = metadata["savings"]
    assert route is not savings
    assert route["feasible"] is not savings["feasible"]
    assert route["feasible"] == {"kind": "solution", "callback": strict_feasible}
    assert savings["feasible"] == {"kind": "solution", "callback": strict_feasible}


def test_nested_list_metadata_rejects_incomplete_or_mixed_declarations() -> None:
    with pytest.raises(TypeError, match="ListRouteHooks requires"):
        ListRouteHooks(
            depot=RowField("depot"),
            distance=None,  # type: ignore[arg-type]
            feasible=EntityCallback(lambda _route, _values: True),
        )

    with pytest.raises(TypeError, match="ListSavingsHooks requires"):
        ListSavingsHooks(
            depot=RowField("depot"),
            metric_class=None,  # type: ignore[arg-type]
            distance=RowField("distance_matrix"),
            feasible=EntityCallback(lambda _route, _values: True),
        )

    with pytest.raises(TypeError, match="route.depot must be"):
        planning_list_variable(
            element_collection="values",
            route=ListRouteHooks(
                depot="depot",  # type: ignore[arg-type]
                distance=RowField("distance_matrix"),
                feasible=EntityCallback(lambda _route, _values: True),
            ),
        )


def test_nested_list_metadata_rejects_invalid_explicit_callback_arities() -> None:
    with pytest.raises(TypeError, match="route.distance EntityCallback must accept 3"):
        planning_list_variable(
            element_collection="values",
            route=ListRouteHooks(
                depot=RowField("depot"),
                distance=EntityCallback(lambda _route: 0),
                feasible=EntityCallback(lambda _route, _values: True),
            ),
        )

    with pytest.raises(
        TypeError, match="cross_position_distance EntityCallback must accept 4"
    ):
        planning_list_variable(
            element_collection="values",
            cross_position_distance=EntityCallback(lambda _route: 0),
        )


def test_compiled_schema_cache_distinguishes_row_and_solution_field_provenance() -> (
    None
):
    @planning_entity
    class CacheRoute:
        visits = planning_list_variable(
            element_collection="values",
            route=ListRouteHooks(
                depot=RowField("depot"),
                distance=RowField("distance_matrix"),
                feasible=CapacityRouteFeasibility(
                    capacity=RowField("capacity"),
                    demand=RowField("demands"),
                ),
            ),
        )

    @planning_solution()
    class CachePlan:
        routes: list[CacheRoute]

        def __init__(self) -> None:
            self.routes = []
            self.values = [0]
            self.distance_matrix = [[0]]
            self.score = None

    first = _compiled_schema_for_solution(CachePlan())
    metadata = dict(CacheRoute.__solverforge_entity__)  # type: ignore[attr-defined]
    field = dict(metadata["fields"][0])
    field_metadata = dict(field["list_metadata"])
    route = dict(field_metadata["route"])
    route["distance"] = {"kind": "solution_field", "field": "distance_matrix"}
    field_metadata["route"] = route
    field["list_metadata"] = field_metadata
    metadata["fields"] = [field]
    CacheRoute.__solverforge_entity__ = metadata  # type: ignore[attr-defined]

    second = _compiled_schema_for_solution(CachePlan())

    assert first is not second


def test_compiled_schema_cache_distinguishes_explicit_savings_bundle() -> None:
    @planning_entity
    class CacheRoute:
        visits = planning_list_variable(
            element_collection="values",
            route=ListRouteHooks(
                depot=RowField("depot"),
                distance=RowField("distance_matrix"),
                feasible=CapacityRouteFeasibility(
                    capacity=RowField("capacity"),
                    demand=RowField("demands"),
                ),
            ),
        )

    @planning_solution()
    class CachePlan:
        routes: list[CacheRoute]

        def __init__(self) -> None:
            self.routes = []
            self.values = [0]
            self.score = None

    first = _compiled_schema_for_solution(CachePlan())
    metadata = dict(CacheRoute.__solverforge_entity__)  # type: ignore[attr-defined]
    field = dict(metadata["fields"][0])
    field_metadata = dict(field["list_metadata"])
    route = field_metadata["route"]
    field_metadata["savings"] = {
        "depot": {"kind": "row", "field": "depot"},
        "metric_class": {"kind": "row", "field": "metric_class"},
        "distance": {"kind": "row", "field": "distance_matrix"},
        "feasible": {
            "kind": "capacity",
            "capacity": {"kind": "row", "field": "capacity"},
            "demand": {"kind": "row", "field": "demands"},
        },
    }
    field["list_metadata"] = field_metadata
    metadata["fields"] = [field]
    CacheRoute.__solverforge_entity__ = metadata  # type: ignore[attr-defined]

    second = _compiled_schema_for_solution(CachePlan())

    assert field_metadata["route"] is route
    assert first is not second


@problem_fact
class CalendarDay:
    def __init__(self, day: str) -> None:
        self.day = day


@planning_solution()
class EmptyAnnotatedPlan:
    tasks: "list[Task]"
    days: "list[CalendarDay]"

    def __init__(self) -> None:
        self.tasks = []
        self.days = []
        self.score = None


def test_schema_resolves_deferred_annotations_for_empty_collections() -> None:
    schema = build_schema(EmptyAnnotatedPlan())

    assert schema["entities"][0]["type_name"] == "Task"
    assert schema["entities"][0]["collection"] == "tasks"
    assert schema["facts"][0]["type_name"] == "CalendarDay"
    assert schema["facts"][0]["collection"] == "days"


@planning_solution()
class MixedInferencePlan:
    tasks: list[Task]

    def __init__(self, days: list[CalendarDay]) -> None:
        self.tasks = [Task()]
        self.days = days
        self.workers = [0, 1]
        self.score = None


def test_compiled_schema_cache_is_keyed_by_complete_inferred_shape() -> None:
    without_facts = _compiled_schema_for_solution(MixedInferencePlan([]))
    with_facts = _compiled_schema_for_solution(MixedInferencePlan([CalendarDay("MON")]))
    same_fact_shape = _compiled_schema_for_solution(
        MixedInferencePlan([CalendarDay("TUE")])
    )

    assert without_facts is not with_facts
    assert with_facts is same_fact_shape


def test_compiled_schema_cache_is_keyed_by_assignment_metadata_field_source() -> None:
    @planning_solution(
        scalar_groups=[
            scalar_assignment_group(
                "task_assignment",
                entity_class="Task",
                variable_name="worker",
                required_entity_field="required_a",
            )
        ]
    )
    class FieldMetadataPlan:
        tasks: list[Task]

        def __init__(self) -> None:
            self.tasks = [Task()]
            self.workers = [0, 1]
            self.score = None

    first = _compiled_schema_for_solution(FieldMetadataPlan())
    metadata = dict(FieldMetadataPlan.__solverforge_solution__)
    metadata["scalar_groups"] = [
        scalar_assignment_group(
            "task_assignment",
            entity_class="Task",
            variable_name="worker",
            required_entity_field="required_b",
        )
    ]
    FieldMetadataPlan.__solverforge_solution__ = metadata
    second = _compiled_schema_for_solution(FieldMetadataPlan())

    assert first is not second


@pytest.mark.parametrize(
    ("factory", "message"),
    [
        (
            lambda: planning_variable(
                value_range_provider="values",
                nearby_value_candidates=42,  # type: ignore[arg-type]
            ),
            "nearby_value_candidates must be a callable or row field name",
        ),
        (
            lambda: planning_list_variable(
                element_collection="values", element_owner=""
            ),
            "element_owner field name must not be empty",
        ),
        (
            lambda: planning_list_variable(
                element_collection="values",
                cross_position_distance=42,  # type: ignore[arg-type]
            ),
            (
                "cross_position_distance must be RowField, SolutionField, "
                "EntityCallback, or SolutionCallback"
            ),
        ),
    ],
)
def test_metadata_sources_reject_invalid_values(factory: object, message: str) -> None:
    with pytest.raises(TypeError, match=message):
        assert callable(factory)
        factory()
