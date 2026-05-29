from solverforge import (
    ConstraintFactory,
    HardSoftScore,
    SoftScore,
    Solver,
    conflict_repair,
    constraint_provider,
    joiner,
    planning_entity,
    planning_solution,
    planning_variable,
    scalar_group,
)


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


def test_dynamic_equal_self_join_delta_keeps_score_consistent_when_left_has_no_match() -> None:
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


def test_dynamic_grouped_constraint_delta_keeps_score_consistent_for_pillar_change() -> None:
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


def test_dynamic_conflict_repair_selector_accepts_callback_weighted_hard_constraint() -> None:
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
