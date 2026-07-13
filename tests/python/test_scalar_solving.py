import threading
from collections.abc import Iterator

import pytest

from solverforge import (
    ConstraintFactory,
    HardSoftScore,
    ModelValidationError,
    ScalarGroupLimits,
    SoftScore,
    Solver,
    conflict_repair,
    constraint_provider,
    joiner,
    planning_entity,
    planning_solution,
    planning_variable,
    scalar_assignment_group,
    scalar_group,
    shadow_variable_updates,
)
from solverforge.model import build_schema


@planning_entity
class Shift:
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self, nurse: int | None = None) -> None:
        self.nurse = nurse


@constraint_provider
def constraints(factory: ConstraintFactory):
    return [
        factory.for_each(Shift)
        .filter(lambda shift: shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("unassigned")
    ]


@planning_solution(score=HardSoftScore, constraints=constraints)
class Schedule:
    shifts: list[Shift]

    def __init__(self) -> None:
        self.shifts = [Shift(), Shift()]
        self.nurses = [0, 1]
        self.score = None


def test_dynamic_scalar_assignment_solves_python_model() -> None:
    schedule = Solver.solve(Schedule())
    assert [shift.nurse for shift in schedule.shifts] == [0, 0]
    assert schedule.score["levels"][0] == 0


def test_synchronous_solve_rejects_unreadable_candidate_trace_request() -> None:
    schedule = Schedule()

    with pytest.raises(RuntimeError, match="candidate_trace.*SolverManager"):
        Solver.solve(schedule, {"candidate_trace": {"max_entries": 1}})

    assert schedule.score is None
    assert [shift.nurse for shift in schedule.shifts] == [None, None]


def filtered_worker_candidates(task: object) -> list[int]:
    return list(task.allowed_workers)


@planning_entity
class FilteredTask:
    worker = planning_variable(
        value_range_provider="filtered_workers",
        candidate_values=filtered_worker_candidates,
    )

    def __init__(self, allowed_workers: list[int]) -> None:
        self.allowed_workers = allowed_workers
        self.worker: int | None = None


@planning_solution(score=SoftScore)
class FilteredWorkerPlan:
    filtered_tasks: list[FilteredTask]

    def __init__(self) -> None:
        self.filtered_workers = [0, 1]
        self.filtered_tasks = [FilteredTask([1])]
        self.score = None


def test_dynamic_scalar_assignment_uses_row_candidate_values() -> None:
    schema = build_schema(FilteredWorkerPlan())
    field = schema["entities"][0]["fields"][0]
    assert callable(field["candidate_values"])

    plan = Solver.solve(
        FilteredWorkerPlan(), {"phases": [{"type": "construction_heuristic"}]}
    )
    assert plan.filtered_tasks[0].worker == 1


nearby_value_candidate_calls: list[int] = []
nearby_value_candidate_yields: list[int] = []
nearby_value_distance_calls: list[tuple[int, int]] = []


def nearby_worker_values(task: object) -> Iterator[int]:
    nearby_value_candidate_calls.append(task.target)

    def generate() -> Iterator[int]:
        for worker in [1, 2]:
            nearby_value_candidate_yields.append(worker)
            yield worker

    return generate()


def nearby_worker_distance(task: object, worker: int) -> float:
    nearby_value_distance_calls.append((task.target, worker))
    return 0.0 if worker == task.target else 100.0


@planning_entity
class NearbyTask:
    worker = planning_variable(
        value_range_provider="workers",
        nearby_value_candidates=nearby_worker_values,
        nearby_value_distance_meter=nearby_worker_distance,
    )

    def __init__(self, target: int, worker: int) -> None:
        self.target = target
        self.worker = worker


@planning_entity
class MetadataNearbyTask:
    worker = planning_variable(
        value_range_provider="workers",
        nearby_value_candidates="nearby_workers",
        nearby_value_distance_meter="worker_distances",
    )

    def __init__(self) -> None:
        self.target = 2
        self.worker = 0
        self.nearby_workers = [1, 2]
        self.worker_distances = [100.0, 50.0, 0.0]


@constraint_provider
def prefer_nearby_target(factory: ConstraintFactory):
    return [
        factory.for_each(NearbyTask)
        .filter(lambda task: task.worker != task.target)
        .penalize(SoftScore.of(10))
        .named("nearby target")
    ]


@constraint_provider
def prefer_metadata_nearby_target(factory: ConstraintFactory):
    return [
        factory.for_each(MetadataNearbyTask)
        .filter(lambda task: task.worker != task.target)
        .penalize(SoftScore.of(10))
        .named("metadata nearby target")
    ]


@planning_solution(score=SoftScore, constraints=prefer_nearby_target)
class NearbyChangePlan:
    tasks: list[NearbyTask]

    def __init__(self) -> None:
        self.tasks = [NearbyTask(target=2, worker=0)]
        self.workers = [0, 1, 2]
        self.score = None


@planning_solution(score=SoftScore, constraints=prefer_metadata_nearby_target)
class MetadataNearbyChangePlan:
    tasks: list[MetadataNearbyTask]

    def __init__(self) -> None:
        self.tasks = [MetadataNearbyTask()]
        self.workers = [0, 1, 2]
        self.nearby_workers: list[int] = []
        self.worker_distances = [0.0, 0.0, 100.0]
        self.score = None


def test_dynamic_nearby_change_uses_python_candidates_and_distance() -> None:
    schema = build_schema(NearbyChangePlan())
    field = schema["entities"][0]["fields"][0]
    assert callable(field["nearby_value_candidates"])
    assert callable(field["nearby_value_distance_meter"])

    plan = Solver.solve(
        NearbyChangePlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "nearby_change_move_selector",
                        "max_nearby": 1,
                        "entity_class": "NearbyTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert plan.tasks[0].worker == 2
    assert plan.score == Solver.analyze(plan)


def test_dynamic_nearby_change_zero_limit_skips_python_providers() -> None:
    nearby_value_candidate_calls.clear()
    nearby_value_candidate_yields.clear()
    nearby_value_distance_calls.clear()

    plan = Solver.solve(
        NearbyChangePlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "nearby_change_move_selector",
                        "max_nearby": 0,
                        "entity_class": "NearbyTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert nearby_value_candidate_calls == []
    assert nearby_value_candidate_yields == []
    assert nearby_value_distance_calls == []
    assert plan.tasks[0].worker == 0


def test_dynamic_nearby_change_stops_python_iterable_at_candidate_limit() -> None:
    nearby_value_candidate_calls.clear()
    nearby_value_candidate_yields.clear()
    nearby_value_distance_calls.clear()

    Solver.solve(
        NearbyChangePlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "nearby_change_move_selector",
                        "max_nearby": 1,
                        "value_candidate_limit": 1,
                        "entity_class": "NearbyTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert nearby_value_candidate_calls == [2]
    assert nearby_value_candidate_yields == [1]
    assert nearby_value_distance_calls == [(2, 1)]


def test_dynamic_nearby_change_uses_row_metadata() -> None:
    schema = build_schema(MetadataNearbyChangePlan())
    field = schema["entities"][0]["fields"][0]
    assert field["nearby_value_candidates_field"] == "nearby_workers"
    assert field["nearby_value_distance_field"] == "worker_distances"

    plan = Solver.solve(
        MetadataNearbyChangePlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "nearby_change_move_selector",
                        "max_nearby": 1,
                        "entity_class": "MetadataNearbyTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert plan.tasks[0].worker == 2


@planning_entity
class Task:
    worker = planning_variable(value_range_provider="workers")

    def __init__(self, worker: int | None = None) -> None:
        self.worker = worker


@constraint_provider
def prefer_worker_one(factory: ConstraintFactory):
    return [
        factory.for_each(Task)
        .filter(lambda task: task.worker != 1)
        .penalize(SoftScore.of(10))
        .named("prefer worker one")
    ]


@planning_solution(score=SoftScore, constraints=prefer_worker_one)
class WorkerPlan:
    tasks: list[Task]

    def __init__(self) -> None:
        self.tasks = [Task()]
        self.workers = [0, 1]
        self.score = None


def test_dynamic_scalar_local_search_uses_upstream_move_selector() -> None:
    plan = Solver.solve(
        WorkerPlan(),
        {
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
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1]
    assert all(level == 0 for level in plan.score["levels"])


def test_dynamic_limited_neighborhood_wraps_scalar_selector() -> None:
    plan = Solver.solve(
        WorkerPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "limited_neighborhood",
                        "selected_count_limit": 2,
                        "selector": {
                            "type": "change_move_selector",
                            "selection_order": "original",
                            "entity_class": "Task",
                            "variable_name": "worker",
                        },
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1]
    assert all(level == 0 for level in plan.score["levels"])


def test_dynamic_scalar_local_search_uses_default_acceptor_when_omitted() -> None:
    plan = Solver.solve(
        WorkerPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "change_move_selector",
                        "entity_class": "Task",
                        "variable_name": "worker",
                    },
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert plan.score == Solver.analyze(plan)


@planning_entity
class SwapTask:
    worker = planning_variable(value_range_provider="workers")

    def __init__(self, name: str, worker: int) -> None:
        self.name = name
        self.worker = worker


@constraint_provider
def prefer_swapped_workers(factory: ConstraintFactory):
    return [
        factory.for_each(SwapTask)
        .filter(lambda task: task.name == "left" and task.worker != 1)
        .penalize(SoftScore.of(10))
        .named("left worker"),
        factory.for_each(SwapTask)
        .filter(lambda task: task.name == "right" and task.worker != 0)
        .penalize(SoftScore.of(10))
        .named("right worker"),
    ]


@planning_solution(score=SoftScore, constraints=prefer_swapped_workers)
class SwapPlan:
    tasks: list[SwapTask]

    def __init__(self) -> None:
        self.tasks = [SwapTask("left", 0), SwapTask("right", 1)]
        self.workers = [0, 1]
        self.score = None


def test_dynamic_scalar_swap_supports_tabu_acceptor() -> None:
    plan = Solver.solve(
        SwapPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "swap_move_selector",
                        "selection_order": "original",
                        "entity_class": "SwapTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "tabu_search", "move_tabu_size": 2},
                    "forager": {"type": "best_score"},
                    "score_tie_break": "first",
                    "termination": {"step_count_limit": 2},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1, 0]
    assert all(level == 0 for level in plan.score["levels"])


@constraint_provider
def prefer_binary_swapped_workers(factory: ConstraintFactory):
    return [
        factory.for_each(SwapTask)
        .join(SwapTask)
        .filter(
            lambda left, right: left.name == "left"
            and right.name == "right"
            and left.worker != 1
            and right.worker != 0
        )
        .penalize(SoftScore.ONE_SOFT)
        .named("binary swapped workers")
    ]


@planning_solution(score=SoftScore, constraints=prefer_binary_swapped_workers)
class BinarySwapPlan:
    tasks: list[SwapTask]

    def __init__(self) -> None:
        self.tasks = [SwapTask("left", 0), SwapTask("right", 1)]
        self.workers = [0, 1]
        self.score = None


def test_dynamic_scalar_swap_keeps_binary_constraint_score_consistent() -> None:
    plan = Solver.solve(
        BinarySwapPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "swap_move_selector",
                        "selection_order": "original",
                        "entity_class": "SwapTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "score_tie_break": "first",
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1, 0]
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [0]


def nearby_task_indices(task: object) -> list[int]:
    return list(task.nearby_indices)


def nearby_task_distance(left: object, right: object) -> float:
    return 0.0 if right.name == "target" else 100.0


@planning_entity
class NearbySwapTask:
    worker = planning_variable(
        value_range_provider="workers",
        nearby_entity_candidates=nearby_task_indices,
        nearby_entity_distance_meter=nearby_task_distance,
    )

    def __init__(
        self, name: str, worker: int, nearby_indices: list[int] | None = None
    ) -> None:
        self.name = name
        self.worker = worker
        self.nearby_indices = list(nearby_indices or [])


@constraint_provider
def prefer_nearby_swap(factory: ConstraintFactory):
    return [
        factory.for_each(NearbySwapTask)
        .filter(lambda task: task.name == "left" and task.worker != 1)
        .penalize(SoftScore.of(10))
        .named("nearby left"),
        factory.for_each(NearbySwapTask)
        .filter(lambda task: task.name == "target" and task.worker != 0)
        .penalize(SoftScore.of(10))
        .named("nearby target"),
    ]


@planning_solution(score=SoftScore, constraints=prefer_nearby_swap)
class NearbySwapPlan:
    tasks: list[NearbySwapTask]

    def __init__(self) -> None:
        self.tasks = [
            NearbySwapTask("left", 0, [1, 2]),
            NearbySwapTask("decoy", 2),
            NearbySwapTask("target", 1),
        ]
        self.workers = [0, 1, 2]
        self.score = None


@planning_solution(score=SoftScore, constraints=prefer_nearby_swap)
class LowerIndexNearbySwapPlan:
    tasks: list[NearbySwapTask]

    def __init__(self) -> None:
        self.tasks = [
            NearbySwapTask("target", 1),
            NearbySwapTask("decoy", 2),
            NearbySwapTask("left", 0, [0]),
        ]
        self.workers = [0, 1, 2]
        self.score = None


def test_dynamic_nearby_swap_uses_python_candidates_and_distance() -> None:
    schema = build_schema(NearbySwapPlan())
    field = schema["entities"][0]["fields"][0]
    assert callable(field["nearby_entity_candidates"])
    assert callable(field["nearby_entity_distance_meter"])

    plan = Solver.solve(
        NearbySwapPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "nearby_swap_move_selector",
                        "max_nearby": 1,
                        "entity_class": "NearbySwapTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1, 2, 0]
    assert plan.score == Solver.analyze(plan)


def test_dynamic_nearby_swap_accepts_asymmetric_lower_index_candidate() -> None:
    plan = Solver.solve(
        LowerIndexNearbySwapPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "nearby_swap_move_selector",
                        "max_nearby": 1,
                        "entity_class": "NearbySwapTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [0, 2, 1]
    assert plan.score == Solver.analyze(plan)


cursor_candidate_calls: list[str] = []
cursor_candidate_yields: list[tuple[str, int]] = []
cursor_distance_calls: list[tuple[str, str]] = []


def cursor_nearby_candidates(task: object) -> Iterator[int]:
    candidates = task.cursor_candidates()

    def generate() -> Iterator[int]:
        for candidate in candidates:
            cursor_candidate_yields.append((task.name, candidate))
            yield candidate

    return generate()


def cursor_nearby_distance(left: object, right: object) -> float:
    return left.cursor_distance_to(right)


@planning_entity
class CursorTask:
    worker = planning_variable(
        value_range_provider="workers",
        nearby_entity_candidates=cursor_nearby_candidates,
        nearby_entity_distance_meter=cursor_nearby_distance,
    )

    def __init__(self, index: int) -> None:
        self.index = index
        self.name = f"task-{index}"
        self.original_worker = index
        self.worker = index
        self.cursor_shadow = 0

    def cursor_candidates(self) -> list[int]:
        cursor_candidate_calls.append(self.name)
        return [index for index in range(4) if index != self.index]

    def cursor_distance_to(self, other: object) -> float:
        cursor_distance_calls.append((self.name, other.name))
        return float(abs(self.index - other.index))

    def calculate_cursor_shadow(self) -> int:
        return int(self.worker) + 10


@constraint_provider
def cursor_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(CursorTask)
        .filter(lambda task: task.worker == task.original_worker)
        .penalize(SoftScore.ONE_SOFT)
        .named("unchanged cursor task")
    ]


@planning_solution(score=SoftScore, constraints=cursor_constraints)
class CursorPlan:
    tasks: list[CursorTask]

    def __init__(self) -> None:
        self.tasks = [CursorTask(index) for index in range(4)]
        self.workers = list(range(4))
        self.cursor_guard = threading.Lock()
        self.score = None


def update_cursor_shadow(solution: object, entity_index: int) -> dict[str, int]:
    task = solution.tasks[entity_index]
    return {"cursor_shadow": task.calculate_cursor_shadow()}


@planning_solution(
    score=SoftScore,
    constraints=cursor_constraints,
    shadow_updates=shadow_variable_updates(
        list_owner="tasks",
        post_update_listener=update_cursor_shadow,
    ),
)
class CursorShadowPlan:
    tasks: list[CursorTask]

    def __init__(self) -> None:
        self.tasks = [CursorTask(index) for index in range(2)]
        self.workers = list(range(2))
        self.score = None


def test_dynamic_union_does_not_generate_unreached_nearby_swap() -> None:
    cursor_candidate_calls.clear()
    cursor_distance_calls.clear()

    plan = Solver.solve(
        CursorPlan(),
        {
            "random_seed": 1,
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "union_move_selector",
                        "selection_order": "sequential",
                        "selectors": [
                            {
                                "type": "change_move_selector",
                                "selection_order": "original",
                                "entity_class": "CursorTask",
                                "variable_name": "worker",
                            },
                            {
                                "type": "nearby_swap_move_selector",
                                "max_nearby": 1,
                                "entity_class": "CursorTask",
                                "variable_name": "worker",
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "accepted_count", "limit": 1},
                    "termination": {"step_count_limit": 1},
                }
            ],
        },
    )

    assert cursor_candidate_calls == []
    assert cursor_distance_calls == []
    assert plan.tasks[0].worker != plan.tasks[0].original_worker
    assert all(task.worker == task.original_worker for task in plan.tasks[1:])
    assert sum(task.worker != task.original_worker for task in plan.tasks) == 1
    assert plan.score["levels"] == [-3]


def test_dynamic_limited_neighborhood_opens_one_nearby_swap_source() -> None:
    cursor_candidate_calls.clear()
    cursor_distance_calls.clear()

    plan = Solver.solve(
        CursorPlan(),
        {
            "random_seed": 1,
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "limited_neighborhood",
                        "selected_count_limit": 1,
                        "selector": {
                            "type": "nearby_swap_move_selector",
                            "max_nearby": 1,
                            "entity_class": "CursorTask",
                            "variable_name": "worker",
                        },
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ],
        },
    )

    assert len(cursor_candidate_calls) == 1
    assert len(cursor_distance_calls) == 3
    assert sum(task.worker != task.original_worker for task in plan.tasks) == 2
    assert plan.score["levels"] == [-2]


def test_dynamic_nearby_cursor_preserves_authored_callback_objects() -> None:
    cursor_candidate_calls.clear()
    cursor_candidate_yields.clear()
    cursor_distance_calls.clear()

    plan = Solver.solve(
        CursorPlan(),
        {
            "random_seed": 1,
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "nearby_swap_move_selector",
                        "max_nearby": 1,
                        "entity_class": "CursorTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ],
        },
    )

    assert len(cursor_candidate_calls) == 4
    assert len(cursor_candidate_yields) == 12
    assert len(cursor_distance_calls) == 12
    assert all(name.startswith("task-") for name in cursor_candidate_calls)
    assert sum(task.worker != task.original_worker for task in plan.tasks) == 2
    assert plan.score["levels"] == [-2]


def test_dynamic_nearby_swap_zero_limit_skips_python_providers() -> None:
    cursor_candidate_calls.clear()
    cursor_candidate_yields.clear()
    cursor_distance_calls.clear()

    plan = Solver.solve(
        CursorPlan(),
        {
            "random_seed": 1,
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "nearby_swap_move_selector",
                        "max_nearby": 0,
                        "entity_class": "CursorTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ],
        },
    )

    assert cursor_candidate_calls == []
    assert cursor_candidate_yields == []
    assert cursor_distance_calls == []
    assert [task.worker for task in plan.tasks] == [0, 1, 2, 3]


def test_dynamic_cartesian_cursor_preserves_authored_callback_objects() -> None:
    cursor_candidate_calls.clear()
    cursor_distance_calls.clear()

    problem = CursorPlan()
    cursor_guard = problem.cursor_guard
    plan = Solver.solve(
        problem,
        {
            "random_seed": 1,
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "cartesian_product_move_selector",
                        "selectors": [
                            {
                                "type": "change_move_selector",
                                "entity_class": "CursorTask",
                                "variable_name": "worker",
                            },
                            {
                                "type": "nearby_swap_move_selector",
                                "max_nearby": 1,
                                "entity_class": "CursorTask",
                                "variable_name": "worker",
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ],
        },
    )

    assert cursor_candidate_calls
    assert cursor_distance_calls
    assert all(name.startswith("task-") for name in cursor_candidate_calls)
    assert plan.cursor_guard is cursor_guard
    assert plan.score == Solver.analyze(plan)


def test_dynamic_cartesian_preview_preserves_class_backed_shadow_callbacks() -> None:
    plan = Solver.solve(
        CursorShadowPlan(),
        {
            "random_seed": 1,
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "cartesian_product_move_selector",
                        "selectors": [
                            {
                                "type": "change_move_selector",
                                "entity_class": "CursorTask",
                                "variable_name": "worker",
                            },
                            {
                                "type": "change_move_selector",
                                "entity_class": "CursorTask",
                                "variable_name": "worker",
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ],
        },
    )

    assert [task.cursor_shadow for task in plan.tasks] == [
        task.worker + 10 for task in plan.tasks
    ]
    assert plan.score == Solver.analyze(plan)


@planning_entity
class AsymmetricJoinTask:
    worker = planning_variable(value_range_provider="workers")

    def __init__(self, name: str, worker: int, target: int) -> None:
        self.name = name
        self.worker = worker
        self.target = target


@constraint_provider
def asymmetric_self_join_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(AsymmetricJoinTask)
        .join(
            AsymmetricJoinTask,
            joiner.equal_bi(lambda left: left.worker, lambda right: right.target),
        )
        .filter(lambda left, right: left.name != right.name)
        .penalize(SoftScore.ONE_SOFT)
        .named("asymmetric match")
    ]


@planning_solution(score=SoftScore, constraints=asymmetric_self_join_constraints)
class AsymmetricSelfJoinPlan:
    tasks: list[AsymmetricJoinTask]

    def __init__(self) -> None:
        self.tasks = [
            AsymmetricJoinTask("red", 0, 0),
            AsymmetricJoinTask("blue", 0, 99),
        ]
        self.workers = [0, 1]
        self.score = None


def test_dynamic_equal_self_join_delta_keeps_score_consistent_when_left_has_no_match() -> (
    None
):
    plan = Solver.solve(
        AsymmetricSelfJoinPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "change_move_selector",
                        "entity_class": "AsymmetricJoinTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert plan.score == Solver.analyze(plan)


@planning_entity
class PillarTask:
    worker = planning_variable(value_range_provider="workers")

    def __init__(self, target: int, worker: int) -> None:
        self.target = target
        self.worker = worker


@constraint_provider
def prefer_pillar_targets(factory: ConstraintFactory):
    return [
        factory.for_each(PillarTask)
        .filter(lambda task: task.worker != task.target)
        .penalize(SoftScore.of(10))
        .named("pillar target")
    ]


@planning_solution(score=SoftScore, constraints=prefer_pillar_targets)
class PillarChangePlan:
    tasks: list[PillarTask]

    def __init__(self) -> None:
        self.tasks = [
            PillarTask(1, 0),
            PillarTask(1, 0),
            PillarTask(1, 1),
            PillarTask(1, 1),
        ]
        self.workers = [0, 1]
        self.score = None


@planning_solution(score=SoftScore, constraints=prefer_pillar_targets)
class PillarSwapPlan:
    tasks: list[PillarTask]

    def __init__(self) -> None:
        self.tasks = [
            PillarTask(1, 0),
            PillarTask(1, 0),
            PillarTask(0, 1),
            PillarTask(0, 1),
        ]
        self.workers = [0, 1]
        self.score = None


@planning_solution(score=SoftScore, constraints=prefer_pillar_targets)
class RuinRecreatePlan:
    tasks: list[PillarTask]

    def __init__(self) -> None:
        self.tasks = [PillarTask(0, 1), PillarTask(0, 1)]
        self.workers = [0, 1]
        self.score = None


def test_dynamic_scalar_pillar_change_selector_solves_python_model() -> None:
    plan = Solver.solve(
        PillarChangePlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "pillar_change_move_selector",
                        "selection_order": "original",
                        "entity_class": "PillarTask",
                        "variable_name": "worker",
                        "minimum_sub_pillar_size": 0,
                        "maximum_sub_pillar_size": 0,
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "score_tie_break": "first",
                    "termination": {"step_count_limit": 2},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1, 1, 1, 1]
    assert all(level == 0 for level in plan.score["levels"])


@planning_entity
class GroupedDeltaTask:
    worker = planning_variable(value_range_provider="workers")

    def __init__(self, worker: int) -> None:
        self.worker = worker


@constraint_provider
def grouped_delta_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(GroupedDeltaTask)
        .group_by(lambda task: task.worker)
        .filter(lambda worker, count: worker == 0)
        .penalize(lambda worker, count: SoftScore.of(count * 10))
        .named("avoid zero")
    ]


@planning_solution(score=SoftScore, constraints=grouped_delta_constraints)
class GroupedDeltaPlan:
    tasks: list[GroupedDeltaTask]

    def __init__(self) -> None:
        self.tasks = [GroupedDeltaTask(0), GroupedDeltaTask(0)]
        self.workers = [0, 1]
        self.score = None


def test_dynamic_grouped_constraint_delta_keeps_score_consistent_for_pillar_change() -> (
    None
):
    plan = Solver.solve(
        GroupedDeltaPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "pillar_change_move_selector",
                        "selection_order": "original",
                        "entity_class": "GroupedDeltaTask",
                        "variable_name": "worker",
                        "minimum_sub_pillar_size": 0,
                        "maximum_sub_pillar_size": 0,
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "score_tie_break": "first",
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1, 1]
    assert plan.score == Solver.analyze(plan)
    assert plan.score == {"family": "soft", "levels": [0]}


def test_dynamic_scalar_pillar_swap_selector_solves_python_model() -> None:
    plan = Solver.solve(
        PillarSwapPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "pillar_swap_move_selector",
                        "selection_order": "original",
                        "entity_class": "PillarTask",
                        "variable_name": "worker",
                        "minimum_sub_pillar_size": 0,
                        "maximum_sub_pillar_size": 0,
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "score_tie_break": "first",
                    "termination": {"step_count_limit": 2},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1, 1, 0, 0]
    assert all(level == 0 for level in plan.score["levels"])


def test_dynamic_scalar_ruin_recreate_selector_solves_python_model() -> None:
    plan = Solver.solve(
        RuinRecreatePlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "ruin_recreate_move_selector",
                        "selection_order": "original",
                        "entity_class": "PillarTask",
                        "variable_name": "worker",
                        "min_ruin_count": 2,
                        "max_ruin_count": 2,
                        "moves_per_step": 1,
                        "recreate_heuristic_type": "first_fit",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "score_tie_break": "first",
                    "termination": {"step_count_limit": 2},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [0, 0]
    assert all(level == 0 for level in plan.score["levels"])


@planning_solution(score=SoftScore, constraints=prefer_pillar_targets)
class CartesianPlan:
    tasks: list[PillarTask]

    def __init__(self) -> None:
        self.tasks = [PillarTask(1, 0), PillarTask(1, 0)]
        self.workers = [0, 1]
        self.score = None


def test_dynamic_cartesian_selector_solves_python_model() -> None:
    plan = Solver.solve(
        CartesianPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "cartesian_product_move_selector",
                        "selectors": [
                            {
                                "type": "change_move_selector",
                                "selection_order": "original",
                                "entity_class": "PillarTask",
                                "variable_name": "worker",
                            },
                            {
                                "type": "change_move_selector",
                                "selection_order": "original",
                                "entity_class": "PillarTask",
                                "variable_name": "worker",
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "score_tie_break": "first",
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1, 1]
    assert all(level == 0 for level in plan.score["levels"])


@planning_solution(score=SoftScore, constraints=prefer_pillar_targets)
class NestedCartesianPlan:
    tasks: list[PillarTask]

    def __init__(self) -> None:
        self.tasks = [PillarTask(0, 1), PillarTask(0, 1), PillarTask(0, 1)]
        self.workers = [0, 1]
        self.score = None


def test_dynamic_nested_cartesian_selector_preserves_preview_order() -> None:
    change = {
        "type": "change_move_selector",
        "selection_order": "original",
        "entity_class": "PillarTask",
        "variable_name": "worker",
    }
    plan = Solver.solve(
        NestedCartesianPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "cartesian_product_move_selector",
                        "selectors": [
                            {
                                "type": "cartesian_product_move_selector",
                                "selectors": [change, change],
                            },
                            change,
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "score_tie_break": "first",
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [0, 0, 0]
    assert plan.score == Solver.analyze(plan)
    assert plan.score == {"family": "soft", "levels": [0]}


@planning_entity
class GroupedTask:
    worker = planning_variable(value_range_provider="workers")

    def __init__(self, worker: int, target: int) -> None:
        self.worker = worker
        self.target = target


@constraint_provider
def prefer_grouped_targets(factory: ConstraintFactory):
    return [
        factory.for_each(GroupedTask)
        .filter(lambda task: task.worker != task.target)
        .penalize(SoftScore.of(10))
        .named("grouped target")
    ]


@scalar_group("paired_workers")
def paired_worker_moves(solution, limits):
    del limits
    return [
        {
            "reason": "paired target repair",
            "edits": [
                {
                    "entity": solution.tasks[0],
                    "variable_name": "worker",
                    "to_value": solution.tasks[0].target,
                },
                {
                    "entity": solution.tasks[1],
                    "variable_name": "worker",
                    "to_value": solution.tasks[1].target,
                },
            ],
        }
    ]


@planning_solution(
    score=SoftScore,
    constraints=prefer_grouped_targets,
    scalar_groups=[paired_worker_moves],
)
class GroupedScalarPlan:
    tasks: list[GroupedTask]

    def __init__(self) -> None:
        self.tasks = [GroupedTask(0, 1), GroupedTask(0, 1)]
        self.workers = [0, 1]
        self.score = None


def test_dynamic_grouped_scalar_selector_solves_python_model() -> None:
    plan = Solver.solve(
        GroupedScalarPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "grouped_scalar_move_selector",
                        "group_name": "paired_workers",
                        "max_moves_per_step": 4,
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1, 1]
    assert all(level == 0 for level in plan.score["levels"])


_deferred_group_calls = 0


@scalar_group("deferred_group")
def deferred_group_moves(solution, limits):
    global _deferred_group_calls
    del solution, limits
    _deferred_group_calls += 1
    return []


@planning_solution(
    score=SoftScore,
    constraints=prefer_grouped_targets,
    scalar_groups=[deferred_group_moves],
)
class DeferredGroupPlan:
    tasks: list[GroupedTask]

    def __init__(self, worker: int = 0, target: int = 1) -> None:
        self.tasks = [GroupedTask(worker, target)]
        self.workers = [0, 1]
        self.score = None


def test_group_callback_is_not_opened_by_limited_zero() -> None:
    global _deferred_group_calls
    _deferred_group_calls = 0

    plan = Solver.solve(
        DeferredGroupPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "limited_neighborhood",
                        "selected_count_limit": 0,
                        "selector": {
                            "type": "grouped_scalar_move_selector",
                            "group_name": "deferred_group",
                        },
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert _deferred_group_calls == 0
    assert plan.tasks[0].worker == 0
    assert plan.score == Solver.analyze(plan)


def test_group_callback_is_not_opened_for_unreached_union_branch() -> None:
    global _deferred_group_calls
    _deferred_group_calls = 0

    plan = Solver.solve(
        DeferredGroupPlan(),
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
                                "type": "change_move_selector",
                                "selection_order": "original",
                                "entity_class": "GroupedTask",
                                "variable_name": "worker",
                            },
                            {
                                "type": "grouped_scalar_move_selector",
                                "group_name": "deferred_group",
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "accepted_count", "limit": 1},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert _deferred_group_calls == 0
    assert plan.tasks[0].worker == 1
    assert plan.score == Solver.analyze(plan)


def test_cartesian_does_not_open_group_callback_without_doable_left_move() -> None:
    global _deferred_group_calls
    _deferred_group_calls = 0

    input_plan = DeferredGroupPlan(worker=1, target=1)
    input_plan.workers = [1]
    plan = Solver.solve(
        input_plan,
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "cartesian_product_move_selector",
                        "selectors": [
                            {
                                "type": "change_move_selector",
                                "entity_class": "GroupedTask",
                                "variable_name": "worker",
                            },
                            {
                                "type": "grouped_scalar_move_selector",
                                "group_name": "deferred_group",
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

    assert _deferred_group_calls == 0
    assert plan.tasks[0].worker == 1
    assert plan.score == Solver.analyze(plan)


@planning_entity
class AssignmentShift:
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self) -> None:
        self.nurse: int | None = None


def assignment_shift_required(solution, entity_index):
    return solution.shifts[entity_index].nurse is None


def assignment_shift_capacity_key(solution, entity_index, nurse_index):
    del entity_index
    return solution.nurse_capacity_key[nurse_index]


@constraint_provider
def assignment_shift_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(AssignmentShift)
        .filter(lambda shift: shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("assignment shift unassigned"),
        factory.for_each(AssignmentShift)
        .filter(lambda shift: shift.nurse is not None)
        .group_by(lambda shift: shift.nurse)
        .filter(lambda _nurse, count: count > 1)
        .penalize(lambda _nurse, count: HardSoftScore.of_hard(count - 1))
        .named("assignment shift duplicate nurse"),
    ]


@planning_solution(
    score=HardSoftScore,
    constraints=assignment_shift_constraints,
    scalar_groups=[
        scalar_assignment_group(
            "shift_nurse_assignment",
            entity_class="AssignmentShift",
            variable_name="nurse",
            required_entity=assignment_shift_required,
            capacity_key=assignment_shift_capacity_key,
            sync_solution_before_callbacks=False,
            limits=ScalarGroupLimits(max_moves_per_step=8),
        )
    ],
)
class AssignmentSchedule:
    shifts: list[AssignmentShift]

    def __init__(self) -> None:
        self.shifts = [AssignmentShift(), AssignmentShift()]
        self.nurses = [0, 1]
        self.nurse_capacity_key = {0: 0, 1: 1}
        self.score = None


_assignment_callback_bypass_calls = 0


@scalar_group("untrusted_assignment_callback")
def untrusted_assignment_callback(solution, limits):
    global _assignment_callback_bypass_calls
    del limits
    _assignment_callback_bypass_calls += 1
    return [
        {
            "reason": "attempt assignment callback bypass",
            "edits": [
                {
                    "entity": solution.shifts[0],
                    "variable_name": "nurse",
                    "to_value": 0,
                }
            ],
        }
    ]


_assignment_repair_bypass_calls = 0


@conflict_repair("assignment shift unassigned")
def untrusted_assignment_repair(solution, limits):
    global _assignment_repair_bypass_calls
    del limits
    _assignment_repair_bypass_calls += 1
    return [
        {
            "reason": "attempt assignment repair bypass",
            "edits": [
                {
                    "entity": solution.shifts[0],
                    "variable_name": "nurse",
                    "to_value": 0,
                }
            ],
        }
    ]


@planning_solution(
    score=HardSoftScore,
    constraints=assignment_shift_constraints,
    scalar_groups=[
        scalar_assignment_group(
            "shift_nurse_assignment",
            entity_class="AssignmentShift",
            variable_name="nurse",
            required_entity=assignment_shift_required,
            capacity_key=assignment_shift_capacity_key,
            sync_solution_before_callbacks=False,
            limits=ScalarGroupLimits(max_moves_per_step=8),
        ),
        untrusted_assignment_callback,
    ],
    conflict_repairs=[untrusted_assignment_repair],
)
class AssignmentCallbackBypassSchedule:
    shifts: list[AssignmentShift]

    def __init__(self) -> None:
        self.shifts = [AssignmentShift(), AssignmentShift()]
        self.nurses = [0, 1]
        self.nurse_capacity_key = {0: 0, 1: 1}
        self.score = None


mixed_nearby_candidate_calls: list[int] = []
mixed_nearby_distance_calls: list[tuple[int, int]] = []


def mixed_preference_candidates(task):
    mixed_nearby_candidate_calls.append(task.preference)
    return [1, 0]


def mixed_preference_distance(task, preference):
    mixed_nearby_distance_calls.append((task.preference, preference))
    return 0.0 if preference == 1 else 100.0


@planning_entity
class MixedAssignmentTask:
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)
    preference = planning_variable(
        value_range_provider="preferences",
        nearby_value_candidates=mixed_preference_candidates,
        nearby_value_distance_meter=mixed_preference_distance,
    )

    def __init__(self) -> None:
        self.nurse: int | None = None
        self.preference = 0


def mixed_assignment_required(solution, entity_index):
    return solution.tasks[entity_index].nurse is None


@constraint_provider
def mixed_assignment_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(MixedAssignmentTask)
        .filter(lambda task: task.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("mixed assignment required"),
        factory.for_each(MixedAssignmentTask)
        .filter(lambda task: task.preference != 1)
        .penalize(HardSoftScore.ONE_SOFT)
        .named("mixed preference"),
    ]


@planning_solution(
    score=HardSoftScore,
    constraints=mixed_assignment_constraints,
    scalar_groups=[
        scalar_assignment_group(
            "mixed_nurse_assignment",
            entity_class="MixedAssignmentTask",
            variable_name="nurse",
            required_entity=mixed_assignment_required,
            sync_solution_before_callbacks=False,
        )
    ],
)
class MixedAssignmentSchedule:
    tasks: list[MixedAssignmentTask]

    def __init__(self) -> None:
        self.tasks = [MixedAssignmentTask()]
        self.nurses = [0, 1]
        self.preferences = [0, 1]
        self.score = None


@planning_entity
class FieldMixedAssignmentTask:
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)
    preference = planning_variable(
        value_range_provider="preferences",
        nearby_value_candidates="nearby_preferences",
        nearby_value_distance_meter="preference_distances",
    )

    def __init__(self) -> None:
        self.nurse: int | None = None
        self.preference = 0
        self.nearby_preferences = [1, 0]
        self.preference_distances = [100.0, 0.0]


def field_mixed_assignment_required(solution, entity_index):
    return solution.tasks[entity_index].nurse is None


@constraint_provider
def field_mixed_assignment_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(FieldMixedAssignmentTask)
        .filter(lambda task: task.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("field mixed assignment required"),
        factory.for_each(FieldMixedAssignmentTask)
        .filter(lambda task: task.preference != 1)
        .penalize(HardSoftScore.ONE_SOFT)
        .named("field mixed preference"),
    ]


@planning_solution(
    score=HardSoftScore,
    constraints=field_mixed_assignment_constraints,
    scalar_groups=[
        scalar_assignment_group(
            "field_mixed_nurse_assignment",
            entity_class="FieldMixedAssignmentTask",
            variable_name="nurse",
            required_entity=field_mixed_assignment_required,
            sync_solution_before_callbacks=False,
        )
    ],
)
class FieldMixedAssignmentSchedule:
    tasks: list[FieldMixedAssignmentTask]

    def __init__(self) -> None:
        self.tasks = [FieldMixedAssignmentTask()]
        self.nurses = [0, 1]
        self.preferences = [0, 1]
        self.score = None


two_assignment_group_callback_calls: list[tuple[str, int]] = []


@planning_entity
class TwoAssignmentGroupTask:
    left = planning_variable(value_range_provider="left_values", allows_unassigned=True)
    right = planning_variable(
        value_range_provider="right_values", allows_unassigned=True
    )

    def __init__(self) -> None:
        self.left: int | None = None
        self.right: int | None = None


def left_assignment_required(solution, entity_index):
    return solution.tasks[entity_index].left is None


def right_assignment_required(solution, entity_index):
    return solution.tasks[entity_index].right is None


def left_assignment_capacity_key(_solution, _entity_index, value):
    two_assignment_group_callback_calls.append(("left_capacity", value))
    return value


def right_assignment_capacity_key(_solution, _entity_index, value):
    two_assignment_group_callback_calls.append(("right_capacity", value))
    return value + 10


def left_assignment_value_order(_solution, _entity_index, value):
    two_assignment_group_callback_calls.append(("left_value_order", value))
    return value


def right_assignment_value_order(_solution, _entity_index, value):
    two_assignment_group_callback_calls.append(("right_value_order", value))
    return 1 - value


@constraint_provider
def two_assignment_group_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(TwoAssignmentGroupTask)
        .filter(lambda task: task.left is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("left assignment required"),
        factory.for_each(TwoAssignmentGroupTask)
        .filter(lambda task: task.right is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("right assignment required"),
    ]


@planning_solution(
    score=HardSoftScore,
    constraints=two_assignment_group_constraints,
    scalar_groups=[
        scalar_assignment_group(
            "left_assignment",
            entity_class="TwoAssignmentGroupTask",
            variable_name="left",
            required_entity=left_assignment_required,
            capacity_key=left_assignment_capacity_key,
            value_order=left_assignment_value_order,
        ),
        scalar_assignment_group(
            "right_assignment",
            entity_class="TwoAssignmentGroupTask",
            variable_name="right",
            required_entity=right_assignment_required,
            capacity_key=right_assignment_capacity_key,
            value_order=right_assignment_value_order,
        ),
    ],
)
class TwoAssignmentGroupSchedule:
    tasks: list[TwoAssignmentGroupTask]

    def __init__(self) -> None:
        self.tasks = [TwoAssignmentGroupTask()]
        self.left_values = [0, 1]
        self.right_values = [0, 1]
        self.score = None


@planning_entity
class FieldAssignmentShift:
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self, required: bool, capacity_keys: list[int], position: int) -> None:
        self.required = required
        self.capacity_keys = capacity_keys
        self.position = position
        self.sequence = position
        self.nurse: int | None = None


def field_assignment_rule(
    _solution, _left_entity, _left_nurse, _right_entity, _right_nurse
):
    return True


@constraint_provider
def field_assignment_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(FieldAssignmentShift)
        .filter(lambda shift: shift.required and shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("field assignment shift unassigned"),
        factory.for_each(FieldAssignmentShift)
        .filter(lambda shift: shift.nurse is not None)
        .group_by(lambda shift: shift.nurse)
        .filter(lambda _nurse, count: count > 1)
        .penalize(lambda _nurse, count: HardSoftScore.of_hard(count - 1))
        .named("field assignment shift duplicate nurse"),
    ]


@planning_solution(
    score=HardSoftScore,
    constraints=field_assignment_constraints,
    scalar_groups=[
        scalar_assignment_group(
            "field_shift_nurse_assignment",
            entity_class="FieldAssignmentShift",
            variable_name="nurse",
            required_entity_field="required",
            capacity_key_field="capacity_keys",
            assignment_rule=field_assignment_rule,
            position_key_field="position",
            sequence_key_field="sequence",
            sync_solution_before_callbacks=False,
        )
    ],
)
class FieldAssignmentSchedule:
    shifts: list[FieldAssignmentShift]

    def __init__(self, capacity_keys: list[int] | None = None) -> None:
        keys = [0, 1] if capacity_keys is None else capacity_keys
        self.shifts = [
            FieldAssignmentShift(True, keys, 0),
            FieldAssignmentShift(True, keys, 1),
        ]
        self.nurses = [0, 1]
        self.score = None


rich_assignment_metadata_calls: set[str] = set()


@planning_entity
class RichAssignmentShift:
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self, ordinal: int) -> None:
        self.ordinal = ordinal
        self.nurse: int | None = None


def rich_assignment_required(_solution, _entity_index):
    rich_assignment_metadata_calls.add("required")
    return True


def rich_assignment_capacity_key(_solution, _entity_index, nurse_idx):
    rich_assignment_metadata_calls.add("capacity")
    return nurse_idx


def rich_assignment_position_key(_solution, entity_index):
    rich_assignment_metadata_calls.add("position")
    return {2: 0, 0: 1, 1: 2}[entity_index]


def rich_assignment_sequence_key(_solution, entity_index, _nurse_idx):
    rich_assignment_metadata_calls.add("sequence")
    return entity_index


def rich_assignment_entity_order(_solution, entity_index):
    rich_assignment_metadata_calls.add("entity_order")
    return {1: 0, 2: 1, 0: 2}[entity_index]


def rich_assignment_value_order(_solution, _entity_index, nurse_idx):
    rich_assignment_metadata_calls.add("value_order")
    return {1: 0, 0: 1}[nurse_idx]


def rich_assignment_rule(_solution, left_entity, left_nurse, right_entity, right_nurse):
    rich_assignment_metadata_calls.add("assignment_rule")
    return left_nurse != right_nurse or abs(left_entity - right_entity) != 1


@constraint_provider
def rich_assignment_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(RichAssignmentShift)
        .filter(lambda shift: shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("rich assignment required"),
        factory.for_each(RichAssignmentShift)
        .filter(lambda shift: shift.nurse is not None)
        .group_by(lambda shift: shift.nurse)
        .filter(lambda _nurse, count: count > 1)
        .penalize(lambda _nurse, count: HardSoftScore.of_hard(count - 1))
        .named("rich assignment duplicate nurse"),
    ]


@planning_solution(
    score=HardSoftScore,
    constraints=rich_assignment_constraints,
    scalar_groups=[
        scalar_assignment_group(
            "rich_assignment",
            entity_class="RichAssignmentShift",
            variable_name="nurse",
            required_entity=rich_assignment_required,
            capacity_key=rich_assignment_capacity_key,
            assignment_rule=rich_assignment_rule,
            position_key=rich_assignment_position_key,
            sequence_key=rich_assignment_sequence_key,
            entity_order=rich_assignment_entity_order,
            value_order=rich_assignment_value_order,
            sync_solution_before_callbacks=False,
            limits=ScalarGroupLimits(max_augmenting_depth=0),
        )
    ],
)
class RichAssignmentSchedule:
    shifts: list[RichAssignmentShift]

    def __init__(self) -> None:
        self.shifts = [
            RichAssignmentShift(0),
            RichAssignmentShift(1),
            RichAssignmentShift(2),
        ]
        self.nurses = [0, 1]
        self.score = None


_counting_assignment_candidate_calls = 0


def counting_assignment_candidates(shift):
    global _counting_assignment_candidate_calls
    _counting_assignment_candidate_calls += 1
    return [shift.shift_index]


@planning_entity
class CountingAssignmentShift:
    nurse = planning_variable(
        value_range_provider="nurses",
        candidate_values=counting_assignment_candidates,
        allows_unassigned=True,
    )

    def __init__(self, shift_index: int) -> None:
        self.shift_index = shift_index
        self.nurse: int | None = None


def counting_assignment_required(solution, entity_index):
    return solution.shifts[entity_index].nurse is None


@constraint_provider
def counting_assignment_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(CountingAssignmentShift)
        .filter(lambda shift: shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("counting assignment shift unassigned")
    ]


@planning_solution(
    score=HardSoftScore,
    constraints=counting_assignment_constraints,
    scalar_groups=[
        scalar_assignment_group(
            "counting_shift_nurse_assignment",
            entity_class="CountingAssignmentShift",
            variable_name="nurse",
            required_entity=counting_assignment_required,
            sync_solution_before_callbacks=False,
        )
    ],
)
class CountingAssignmentSchedule:
    shifts: list[CountingAssignmentShift]

    def __init__(self, shift_count: int) -> None:
        self.shifts = [
            CountingAssignmentShift(shift_index) for shift_index in range(shift_count)
        ]
        self.nurses = list(range(shift_count))
        self.score = None


@planning_entity
class OptionalAssignmentShift:
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self, required: bool) -> None:
        self.required = required
        self.nurse: int | None = None


def optional_assignment_required(solution, entity_index):
    return bool(solution.shifts[entity_index].required)


@constraint_provider
def optional_assignment_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(OptionalAssignmentShift)
        .filter(lambda shift: shift.required and shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("required assignment shift unassigned"),
        factory.for_each(OptionalAssignmentShift)
        .filter(lambda shift: not shift.required and shift.nurse is not None)
        .penalize(HardSoftScore.ONE_SOFT)
        .named("optional assignment shift assigned"),
    ]


@planning_solution(
    score=HardSoftScore,
    constraints=optional_assignment_constraints,
    scalar_groups=[
        scalar_assignment_group(
            "optional_shift_nurse_assignment",
            entity_class="OptionalAssignmentShift",
            variable_name="nurse",
            required_entity=optional_assignment_required,
            sync_solution_before_callbacks=False,
            limits=ScalarGroupLimits(max_moves_per_step=8),
        )
    ],
)
class OptionalAssignmentSchedule:
    shifts: list[OptionalAssignmentShift]

    def __init__(self) -> None:
        self.shifts = [OptionalAssignmentShift(True), OptionalAssignmentShift(False)]
        self.nurses = [0, 1]
        self.score = None


@planning_solution(
    score=HardSoftScore,
    constraints=assignment_shift_constraints,
    scalar_groups=[
        scalar_assignment_group(
            "bad_shift_nurse_assignment",
            entity_class="AssignmentShift",
            variable_name="missing_nurse",
            required_entity=assignment_shift_required,
        )
    ],
)
class BadTargetAssignmentSchedule:
    shifts: list[AssignmentShift]

    def __init__(self) -> None:
        self.shifts = [AssignmentShift(), AssignmentShift()]
        self.nurses = [0, 1]
        self.nurse_capacity_key = {0: 0, 1: 1}
        self.score = None


def broken_assignment_required(_solution, _entity_index):
    raise RuntimeError("assignment metadata exploded")


@planning_solution(
    score=HardSoftScore,
    constraints=assignment_shift_constraints,
    scalar_groups=[
        scalar_assignment_group(
            "broken_shift_nurse_assignment",
            entity_class="AssignmentShift",
            variable_name="nurse",
            required_entity=broken_assignment_required,
            sync_solution_before_callbacks=False,
        )
    ],
)
class BrokenAssignmentSchedule:
    shifts: list[AssignmentShift]

    def __init__(self) -> None:
        self.shifts = [AssignmentShift()]
        self.nurses = [0]
        self.nurse_capacity_key = {0: 0}
        self.score = None


@planning_solution(
    score=HardSoftScore,
    constraints=assignment_shift_constraints,
    scalar_groups=[
        scalar_assignment_group(
            "duplicate_shift_nurse_assignment",
            entity_class="AssignmentShift",
            variable_name="nurse",
        ),
        scalar_assignment_group(
            "duplicate_shift_nurse_assignment",
            entity_class="AssignmentShift",
            variable_name="nurse",
        ),
    ],
)
class DuplicateAssignmentGroupSchedule:
    shifts: list[AssignmentShift]

    def __init__(self) -> None:
        self.shifts = [AssignmentShift(), AssignmentShift()]
        self.nurses = [0, 1]
        self.score = None


def test_scalar_assignment_group_schema_exposes_public_contract() -> None:
    schema = build_schema(AssignmentSchedule())
    group = schema["scalar_groups"][0]

    assert group["kind"] == "assignment"
    assert group["name"] == "shift_nurse_assignment"
    assert group["entity_class"] == "AssignmentShift"
    assert group["variable_name"] == "nurse"
    assert callable(group["required_entity"])
    assert callable(group["capacity_key"])
    assert group["sync_solution_before_callbacks"] is False
    assert group["limits"]["max_moves_per_step"] == 8


def test_scalar_assignment_group_field_metadata_solves_with_rule_callback() -> None:
    schema = build_schema(FieldAssignmentSchedule())
    group = schema["scalar_groups"][0]

    assert group["required_entity"] is None
    assert group["required_entity_field"] == "required"
    assert group["capacity_key"] is None
    assert group["capacity_key_field"] == "capacity_keys"
    assert group["position_key"] is None
    assert group["position_key_field"] == "position"
    assert group["sequence_key"] is None
    assert group["sequence_key_field"] == "sequence"

    plan = Solver.solve(
        FieldAssignmentSchedule(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "cheapest_insertion",
                    "group_name": "field_shift_nurse_assignment",
                }
            ]
        },
    )

    assert sorted(shift.nurse for shift in plan.shifts) == [0, 1]
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [0, 0]


def test_scalar_assignment_group_field_metadata_rejects_missing_candidate_capacity() -> (
    None
):
    with pytest.raises(RuntimeError, match="capacity_keys.*candidate 1"):
        Solver.solve(
            FieldAssignmentSchedule([0]),
            {
                "phases": [
                    {
                        "type": "construction_heuristic",
                        "group_name": "field_shift_nurse_assignment",
                    }
                ]
            },
        )


def test_scalar_assignment_group_callback_metadata_preserves_rich_core_ordering() -> (
    None
):
    rich_assignment_metadata_calls.clear()

    plan = Solver.solve(
        RichAssignmentSchedule(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "first_fit",
                    "construction_obligation": "assign_when_candidate_exists",
                    "group_name": "rich_assignment",
                }
            ]
        },
    )

    assert [shift.nurse for shift in plan.shifts] == [None, 1, 0]
    assert {
        "required",
        "capacity",
        "sequence",
        "entity_order",
        "value_order",
        "assignment_rule",
    } <= rich_assignment_metadata_calls
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [-1, 0]

    rich_assignment_metadata_calls.clear()
    plan = Solver.solve(
        plan,
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "grouped_scalar_move_selector",
                        "group_name": "rich_assignment",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert "position" in rich_assignment_metadata_calls
    assert plan.score == Solver.analyze(plan)


def test_scalar_assignment_group_rejects_callback_and_field_for_same_metadata() -> None:
    with pytest.raises(TypeError, match="required_entity and required_entity_field"):
        scalar_assignment_group(
            "invalid_field_assignment_group",
            entity_class="FieldAssignmentShift",
            variable_name="nurse",
            required_entity=lambda _solution, _entity_index: True,
            required_entity_field="required",
        )


def test_grouped_scalar_assignment_selector_solves_python_model() -> None:
    plan = Solver.solve(
        AssignmentSchedule(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "grouped_scalar_move_selector",
                        "group_name": "shift_nurse_assignment",
                        "max_moves_per_step": 8,
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 2},
                }
            ]
        },
    )

    assert sorted(shift.nurse for shift in plan.shifts) == [0, 1]
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [0, 0]


def test_scalar_assignment_group_construction_solves_python_model() -> None:
    plan = Solver.solve(
        AssignmentSchedule(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "cheapest_insertion",
                    "group_name": "shift_nurse_assignment",
                }
            ]
        },
    )

    assert sorted(shift.nurse for shift in plan.shifts) == [0, 1]
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [0, 0]


def test_assignment_group_metadata_binds_to_each_declared_group() -> None:
    two_assignment_group_callback_calls.clear()

    plan = Solver.solve(
        TwoAssignmentGroupSchedule(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "weakest_fit",
                    "group_name": "left_assignment",
                },
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "weakest_fit",
                    "group_name": "right_assignment",
                },
            ]
        },
    )

    assert plan.tasks[0].left == 0
    assert plan.tasks[0].right == 1
    assert {
        callback_name for callback_name, _value in two_assignment_group_callback_calls
    } == {
        "left_capacity",
        "left_value_order",
        "right_capacity",
        "right_value_order",
    }
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [0, 0]


@pytest.mark.parametrize(
    "construction_heuristic_type", ["first_fit", "cheapest_insertion"]
)
def test_scalar_assignment_group_explicit_construction_obeys_expired_step_limit(
    construction_heuristic_type: str,
) -> None:
    plan = Solver.solve(
        AssignmentSchedule(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": construction_heuristic_type,
                    "group_name": "shift_nurse_assignment",
                    "termination": {"step_count_limit": 0},
                }
            ]
        },
    )

    assert [shift.nurse for shift in plan.shifts] == [None, None]
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [-2, 0]


def test_scalar_assignment_group_default_construction_completes_required_assignments() -> (
    None
):
    plan = Solver.solve(AssignmentSchedule())

    assert sorted(shift.nurse for shift in plan.shifts) == [0, 1]
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [0, 0]


def test_scalar_assignment_group_bounds_core_required_candidate_callbacks() -> None:
    global _counting_assignment_candidate_calls
    _counting_assignment_candidate_calls = 0
    plan = Solver.solve(
        CountingAssignmentSchedule(12),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "first_fit",
                    "group_name": "counting_shift_nurse_assignment",
                }
            ]
        },
    )

    assert [shift.nurse for shift in plan.shifts] == list(range(12))
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [0, 0]
    assert _counting_assignment_candidate_calls <= 2 * len(plan.shifts) + 1


def test_scalar_assignment_group_construction_preserves_solution_context_in_previews() -> (
    None
):
    plan = Solver.solve(
        AssignmentSchedule(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "cheapest_insertion",
                    "group_name": "shift_nurse_assignment",
                }
            ]
        },
    )

    assert sorted(shift.nurse for shift in plan.shifts) == [0, 1]
    assert plan.nurse_capacity_key == {0: 0, 1: 1}


def test_scalar_assignment_group_construction_skips_soft_worse_optional_rows() -> None:
    plan = Solver.solve(
        OptionalAssignmentSchedule(),
        {
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "cheapest_insertion",
                    "group_name": "optional_shift_nurse_assignment",
                }
            ]
        },
    )

    assert plan.shifts[0].nurse is not None
    assert plan.shifts[1].nurse is None
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [0, 0]


def test_scalar_assignment_group_construction_reports_unknown_group() -> None:
    with pytest.raises(RuntimeError, match="no matching assignment scalar group"):
        Solver.solve(
            AssignmentSchedule(),
            {
                "phases": [
                    {
                        "type": "construction_heuristic",
                        "construction_heuristic_type": "cheapest_insertion",
                        "group_name": "missing_shift_nurse_assignment",
                    }
                ]
            },
        )


def test_scalar_assignment_group_construction_reports_unknown_target() -> None:
    with pytest.raises(RuntimeError, match="targets unknown scalar variable"):
        Solver.solve(
            BadTargetAssignmentSchedule(),
            {
                "phases": [
                    {
                        "type": "construction_heuristic",
                        "construction_heuristic_type": "cheapest_insertion",
                        "group_name": "bad_shift_nurse_assignment",
                    }
                ]
            },
        )


def test_scalar_assignment_group_duplicate_names_fail_schema_build() -> None:
    with pytest.raises(ModelValidationError, match="declared more than once"):
        build_schema(DuplicateAssignmentGroupSchedule())


def test_assignment_group_callback_exception_preserves_python_error() -> None:
    with pytest.raises(RuntimeError, match="assignment metadata exploded"):
        Solver.solve(
            BrokenAssignmentSchedule(),
            {
                "phases": [
                    {
                        "type": "construction_heuristic",
                        "group_name": "broken_shift_nurse_assignment",
                    }
                ]
            },
        )


def test_grouped_scalar_assignment_selector_composes_in_union() -> None:
    plan = Solver.solve(
        TwoAssignmentGroupSchedule(),
        {
            "random_seed": 1,
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "union_move_selector",
                        "selectors": [
                            {
                                "type": "grouped_scalar_move_selector",
                                "selection_order": "original",
                                "group_name": "left_assignment",
                                "max_moves_per_step": 8,
                            },
                            {
                                "type": "grouped_scalar_move_selector",
                                "selection_order": "original",
                                "group_name": "right_assignment",
                                "max_moves_per_step": 8,
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "score_tie_break": "first",
                    "termination": {"step_count_limit": 2},
                }
            ],
        },
    )

    assert plan.tasks[0].left == 0
    assert plan.tasks[0].right == 1
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [0, 0]


def test_grouped_scalar_assignment_selector_rejects_raw_same_target_in_union() -> None:
    with pytest.raises(RuntimeError, match="is assignment-owned"):
        Solver.solve(
            AssignmentSchedule(),
            {
                "phases": [
                    {
                        "type": "local_search",
                        "local_search_type": "acceptor_forager",
                        "move_selector": {
                            "type": "union_move_selector",
                            "selectors": [
                                {
                                    "type": "grouped_scalar_move_selector",
                                    "group_name": "shift_nurse_assignment",
                                    "max_moves_per_step": 8,
                                },
                                {
                                    "type": "change_move_selector",
                                    "entity_class": "AssignmentShift",
                                    "variable_name": "nurse",
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


@pytest.mark.parametrize(
    "move_selector",
    [
        {"type": "change_move_selector"},
        {
            "type": "change_move_selector",
            "entity_class": "AssignmentShift",
            "variable_name": "nurse",
        },
        {
            "type": "union_move_selector",
            "selectors": [
                {
                    "type": "change_move_selector",
                    "entity_class": "AssignmentShift",
                    "variable_name": "nurse",
                }
            ],
        },
        {
            "type": "limited_neighborhood",
            "selected_count_limit": 1,
            "selector": {
                "type": "change_move_selector",
                "entity_class": "AssignmentShift",
                "variable_name": "nurse",
            },
        },
    ],
)
def test_assignment_owned_scalar_rejects_raw_wrapper_local_search(
    move_selector: dict[str, object],
) -> None:
    with pytest.raises(RuntimeError, match="is assignment-owned"):
        Solver.solve(
            AssignmentSchedule(),
            {
                "phases": [
                    {
                        "type": "local_search",
                        "local_search_type": "acceptor_forager",
                        "move_selector": move_selector,
                        "acceptor": {"type": "hill_climbing"},
                        "forager": {"type": "best_score"},
                        "termination": {"step_count_limit": 1},
                    }
                ]
            },
        )


def test_assignment_owned_scalar_rejects_implicit_all_slot_callback_group() -> None:
    global _assignment_callback_bypass_calls
    _assignment_callback_bypass_calls = 0

    with pytest.raises(RuntimeError, match="is assignment-owned"):
        Solver.solve(
            AssignmentCallbackBypassSchedule(),
            {
                "phases": [
                    {
                        "type": "local_search",
                        "local_search_type": "acceptor_forager",
                        "move_selector": {
                            "type": "grouped_scalar_move_selector",
                            "group_name": "untrusted_assignment_callback",
                        },
                        "acceptor": {"type": "hill_climbing"},
                        "forager": {"type": "best_score"},
                        "termination": {"step_count_limit": 1},
                    }
                ]
            },
        )

    assert _assignment_callback_bypass_calls == 0


@pytest.mark.parametrize(
    "move_selector",
    [
        {
            "type": "limited_neighborhood",
            "selected_count_limit": 0,
            "selector": {
                "type": "grouped_scalar_move_selector",
                "group_name": "untrusted_assignment_callback",
            },
        },
        {
            "type": "union_move_selector",
            "selectors": [
                {
                    "type": "limited_neighborhood",
                    "selected_count_limit": 0,
                    "selector": {
                        "type": "grouped_scalar_move_selector",
                        "group_name": "untrusted_assignment_callback",
                    },
                },
                {
                    "type": "change_move_selector",
                    "entity_class": "AssignmentShift",
                    "variable_name": "nurse",
                },
            ],
        },
    ],
)
def test_assignment_owned_scalar_rejects_unscoped_group_before_nested_callback_pull(
    move_selector: dict[str, object],
) -> None:
    global _assignment_callback_bypass_calls
    _assignment_callback_bypass_calls = 0

    with pytest.raises(RuntimeError, match="is assignment-owned"):
        Solver.solve(
            AssignmentCallbackBypassSchedule(),
            {
                "phases": [
                    {
                        "type": "local_search",
                        "local_search_type": "acceptor_forager",
                        "move_selector": move_selector,
                        "acceptor": {"type": "hill_climbing"},
                        "forager": {"type": "best_score"},
                        "termination": {"step_count_limit": 1},
                    }
                ]
            },
        )

    assert _assignment_callback_bypass_calls == 0


def test_assignment_owned_scalar_rejects_implicit_all_slot_conflict_repair() -> None:
    global _assignment_repair_bypass_calls
    _assignment_repair_bypass_calls = 0

    with pytest.raises(RuntimeError, match="is assignment-owned"):
        Solver.solve(
            AssignmentCallbackBypassSchedule(),
            {
                "phases": [
                    {
                        "type": "local_search",
                        "local_search_type": "acceptor_forager",
                        "move_selector": {
                            "type": "conflict_repair_move_selector",
                            "constraints": ["assignment shift unassigned"],
                        },
                        "acceptor": {"type": "hill_climbing"},
                        "forager": {"type": "best_score"},
                        "termination": {"step_count_limit": 1},
                    }
                ]
            },
        )

    assert _assignment_repair_bypass_calls == 0


@pytest.mark.parametrize(
    "move_selector",
    [
        {
            "type": "limited_neighborhood",
            "selected_count_limit": 0,
            "selector": {
                "type": "conflict_repair_move_selector",
                "constraints": ["assignment shift unassigned"],
            },
        },
        {
            "type": "union_move_selector",
            "selectors": [
                {
                    "type": "limited_neighborhood",
                    "selected_count_limit": 0,
                    "selector": {
                        "type": "conflict_repair_move_selector",
                        "constraints": ["assignment shift unassigned"],
                    },
                },
                {
                    "type": "change_move_selector",
                    "entity_class": "AssignmentShift",
                    "variable_name": "nurse",
                },
            ],
        },
    ],
)
def test_assignment_owned_scalar_rejects_unscoped_repair_before_nested_callback_pull(
    move_selector: dict[str, object],
) -> None:
    global _assignment_repair_bypass_calls
    _assignment_repair_bypass_calls = 0

    with pytest.raises(RuntimeError, match="is assignment-owned"):
        Solver.solve(
            AssignmentCallbackBypassSchedule(),
            {
                "phases": [
                    {
                        "type": "local_search",
                        "local_search_type": "acceptor_forager",
                        "move_selector": move_selector,
                        "acceptor": {"type": "hill_climbing"},
                        "forager": {"type": "best_score"},
                        "termination": {"step_count_limit": 1},
                    }
                ]
            },
        )

    assert _assignment_repair_bypass_calls == 0


@pytest.mark.parametrize(
    "target",
    [
        {},
        {"entity_class": "AssignmentShift", "variable_name": "nurse"},
    ],
)
def test_assignment_owned_scalar_rejects_ungrouped_construction_target(
    target: dict[str, str],
) -> None:
    with pytest.raises(RuntimeError, match="is assignment-owned"):
        Solver.solve(
            AssignmentSchedule(),
            {
                "phases": [
                    {
                        "type": "construction_heuristic",
                        "construction_heuristic_type": "first_fit",
                        **target,
                    }
                ]
            },
        )


def test_assignment_group_and_nearby_dynamic_selector_share_core_phase() -> None:
    mixed_nearby_candidate_calls.clear()
    mixed_nearby_distance_calls.clear()

    plan = Solver.solve(
        MixedAssignmentSchedule(),
        {
            "random_seed": 1,
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "first_fit",
                    "group_name": "mixed_nurse_assignment",
                },
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "union_move_selector",
                        "selectors": [
                            {
                                "type": "grouped_scalar_move_selector",
                                "group_name": "mixed_nurse_assignment",
                            },
                            {
                                "type": "nearby_change_move_selector",
                                "max_nearby": 1,
                                "entity_class": "MixedAssignmentTask",
                                "variable_name": "preference",
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                },
            ],
        },
    )

    assert plan.tasks[0].nurse is not None
    assert plan.tasks[0].preference == 1
    assert mixed_nearby_candidate_calls == [0]
    assert mixed_nearby_distance_calls == [(0, 1)]
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [0, 0]


def test_assignment_group_core_phase_does_not_open_unreached_nearby_callback() -> None:
    """A core-routed union keeps the dynamic nearby source lazy per selector."""
    mixed_nearby_candidate_calls.clear()
    mixed_nearby_distance_calls.clear()

    plan = Solver.solve(
        MixedAssignmentSchedule(),
        {
            "random_seed": 1,
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "union_move_selector",
                        "selectors": [
                            {
                                "type": "grouped_scalar_move_selector",
                                "group_name": "mixed_nurse_assignment",
                            },
                            {
                                "type": "nearby_change_move_selector",
                                "max_nearby": 1,
                                "entity_class": "MixedAssignmentTask",
                                "variable_name": "preference",
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "accepted_count", "limit": 1},
                    "termination": {"step_count_limit": 1},
                }
            ],
        },
    )

    assert plan.tasks[0].nurse is not None
    assert plan.tasks[0].preference == 0
    assert mixed_nearby_candidate_calls == []
    assert mixed_nearby_distance_calls == []
    assert plan.score == Solver.analyze(plan)


def test_assignment_group_core_phase_reads_nearby_row_metadata() -> None:
    """Field-backed nearby metadata follows the same core dynamic slot path."""
    plan = Solver.solve(
        FieldMixedAssignmentSchedule(),
        {
            "random_seed": 1,
            "phases": [
                {
                    "type": "construction_heuristic",
                    "construction_heuristic_type": "first_fit",
                    "group_name": "field_mixed_nurse_assignment",
                },
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "union_move_selector",
                        "selectors": [
                            {
                                "type": "grouped_scalar_move_selector",
                                "group_name": "field_mixed_nurse_assignment",
                            },
                            {
                                "type": "nearby_change_move_selector",
                                "max_nearby": 1,
                                "entity_class": "FieldMixedAssignmentTask",
                                "variable_name": "preference",
                            },
                        ],
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                },
            ],
        },
    )

    assert plan.tasks[0].nurse is not None
    assert plan.tasks[0].preference == 1
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [0, 0]


@planning_entity
class RepairTask:
    worker = planning_variable(value_range_provider="workers")

    def __init__(self, name: str, worker: int, target: int) -> None:
        self.name = name
        self.worker = worker
        self.target = target


@constraint_provider
def hard_repair_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(RepairTask)
        .filter(lambda task: task.worker != task.target)
        .penalize(HardSoftScore.ONE_HARD)
        .named("wrong worker")
    ]


@constraint_provider
def callback_hard_repair_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(RepairTask)
        .filter(lambda task: task.worker != task.target)
        .penalize(lambda task: HardSoftScore.ONE_HARD)
        .named("callback wrong worker")
    ]


@conflict_repair("wrong worker")
def repair_wrong_worker(solution, limits):
    del limits
    return [
        {
            "reason": f"repair {task.name}",
            "edits": [
                {
                    "entity": task,
                    "variable_name": "worker",
                    "to_value": task.target,
                }
            ],
        }
        for task in solution.tasks
        if task.worker != task.target
    ]


@conflict_repair("wrong worker")
def repair_wrong_workers_compound(solution, limits):
    del limits
    return [
        {
            "reason": "repair all workers",
            "edits": [
                {
                    "entity": task,
                    "variable_name": "worker",
                    "to_value": task.target,
                }
                for task in solution.tasks
                if task.worker != task.target
            ],
        }
    ]


@conflict_repair("callback wrong worker")
def repair_callback_wrong_worker(solution, limits):
    del limits
    return [
        {
            "reason": f"repair {task.name}",
            "edits": [
                {
                    "entity": task,
                    "variable_name": "worker",
                    "to_value": task.target,
                }
            ],
        }
        for task in solution.tasks
        if task.worker != task.target
    ]


@planning_solution(
    score=HardSoftScore,
    constraints=hard_repair_constraints,
    conflict_repairs=[repair_wrong_worker],
)
class ConflictRepairPlan:
    tasks: list[RepairTask]

    def __init__(self) -> None:
        self.tasks = [RepairTask("a", 0, 1)]
        self.workers = [0, 1]
        self.score = None


@planning_solution(
    score=HardSoftScore,
    constraints=hard_repair_constraints,
    conflict_repairs=[repair_wrong_workers_compound],
)
class CompoundConflictRepairPlan:
    tasks: list[RepairTask]

    def __init__(self) -> None:
        self.tasks = [RepairTask("a", 0, 1), RepairTask("b", 0, 1)]
        self.workers = [0, 1]
        self.score = None


@planning_solution(
    score=HardSoftScore,
    constraints=callback_hard_repair_constraints,
    conflict_repairs=[repair_callback_wrong_worker],
)
class CallbackHardConflictRepairPlan:
    tasks: list[RepairTask]

    def __init__(self) -> None:
        self.tasks = [RepairTask("a", 0, 1)]
        self.workers = [0, 1]
        self.score = None


def test_dynamic_conflict_repair_selector_solves_python_model() -> None:
    plan = Solver.solve(
        ConflictRepairPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "conflict_repair_move_selector",
                        "constraints": ["wrong worker"],
                        "max_moves_per_step": 4,
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1]
    assert plan.score["levels"][0] == 0


def test_dynamic_conflict_repair_selector_accepts_callback_weighted_hard_constraint() -> (
    None
):
    plan = Solver.solve(
        CallbackHardConflictRepairPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "conflict_repair_move_selector",
                        "constraints": ["callback wrong worker"],
                        "max_moves_per_step": 4,
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1]
    assert plan.score["levels"][0] == 0


def test_dynamic_compound_conflict_repair_selector_solves_python_model() -> None:
    plan = Solver.solve(
        CompoundConflictRepairPlan(),
        {
            "phases": [
                {
                    "type": "local_search",
                    "local_search_type": "acceptor_forager",
                    "move_selector": {
                        "type": "compound_conflict_repair_move_selector",
                        "constraints": ["wrong worker"],
                        "max_moves_per_step": 4,
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
                    "termination": {"step_count_limit": 1},
                }
            ]
        },
    )

    assert [task.worker for task in plan.tasks] == [1, 1]
    assert plan.score["levels"][0] == 0
