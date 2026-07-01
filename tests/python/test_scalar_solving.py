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


def nearby_worker_values(task: object) -> list[int]:
    return [1, 2]


def nearby_worker_distance(task: object, worker: int) -> float:
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


@constraint_provider
def prefer_nearby_target(factory: ConstraintFactory):
    return [
        factory.for_each(NearbyTask)
        .filter(lambda task: task.worker != task.target)
        .penalize(SoftScore.of(10))
        .named("nearby target")
    ]


@planning_solution(score=SoftScore, constraints=prefer_nearby_target)
class NearbyChangePlan:
    tasks: list[NearbyTask]

    def __init__(self) -> None:
        self.tasks = [NearbyTask(target=2, worker=0)]
        self.workers = [0, 1, 2]
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
                        "entity_class": "SwapTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "tabu_search", "move_tabu_size": 2},
                    "forager": {"type": "best_score"},
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
                        "entity_class": "SwapTask",
                        "variable_name": "worker",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
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
                        "entity_class": "PillarTask",
                        "variable_name": "worker",
                        "minimum_sub_pillar_size": 0,
                        "maximum_sub_pillar_size": 0,
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
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
                        "entity_class": "GroupedDeltaTask",
                        "variable_name": "worker",
                        "minimum_sub_pillar_size": 0,
                        "maximum_sub_pillar_size": 0,
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
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
                        "entity_class": "PillarTask",
                        "variable_name": "worker",
                        "minimum_sub_pillar_size": 0,
                        "maximum_sub_pillar_size": 0,
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
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
                        "entity_class": "PillarTask",
                        "variable_name": "worker",
                        "min_ruin_count": 2,
                        "max_ruin_count": 2,
                        "moves_per_step": 1,
                        "recreate_heuristic_type": "first_fit",
                    },
                    "acceptor": {"type": "hill_climbing"},
                    "forager": {"type": "best_score"},
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
                                "entity_class": "PillarTask",
                                "variable_name": "worker",
                            },
                            {
                                "type": "change_move_selector",
                                "entity_class": "PillarTask",
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

    assert [task.worker for task in plan.tasks] == [1, 1]
    assert all(level == 0 for level in plan.score["levels"])


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


@pytest.mark.parametrize(
    "construction_heuristic_type", ["first_fit", "cheapest_insertion"]
)
def test_scalar_assignment_group_construction_completes_required_under_expired_limit(
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

    assert sorted(shift.nurse for shift in plan.shifts) == [0, 1]
    assert plan.score == Solver.analyze(plan)
    assert plan.score["levels"] == [0, 0]


def test_scalar_assignment_group_first_fit_streams_python_required_candidates() -> None:
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
                    "termination": {"step_count_limit": 0},
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


def test_grouped_scalar_assignment_selector_composes_in_union() -> None:
    plan = Solver.solve(
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

    assert sorted(shift.nurse for shift in plan.shifts) == [0, 1]
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
