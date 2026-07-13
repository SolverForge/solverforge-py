from copy import deepcopy
from pathlib import Path

import pytest

from examples.solverforge_hospital import (
    HOSPITAL_SOLVER_CONFIG,
    HospitalPlan,
    assignment_summary,
    demo_plan,
    fixture_payload,
    gap_minutes,
    overlaps,
    plan_from_payload,
    same_day,
    solve_demo,
)
from examples.solverforge_hospital.src.api.dto import plan_to_payload
from examples.solverforge_hospital.src.domain.plan import eligible_employee_candidates
from solverforge import Solver, SolverConfig
from solverforge.model import build_schema

HOSPITAL_EXAMPLE_ROOT = Path(__file__).parents[2] / "examples" / "solverforge_hospital"


def assert_hard_feasible(plan: HospitalPlan) -> None:
    for shift in plan.shifts:
        assert shift.employee_idx is not None
        assert shift.employee_has_skill[shift.employee_idx]
        assert shift.employee_unavailable_minutes[shift.employee_idx] == 0

    for left in plan.shifts:
        for right in plan.shifts:
            if left.index >= right.index:
                continue
            if left.employee_idx != right.employee_idx:
                continue
            assert not overlaps(left, right)
            assert not same_day(left, right)
            gap = gap_minutes(left, right)
            assert gap is None or gap >= 10 * 60


def test_solverforge_hospital_python_model_solves_full_schedule() -> None:
    plan = solve_demo()

    assert_hard_feasible(plan)
    assert plan.score["family"] == "hard_soft_decimal"
    assert plan.score["levels"][0] == 0
    assert plan.score["levels"][1] >= 15_199_994
    assert len(assignment_summary(plan)) == 688


def test_solverforge_hospital_construction_matches_upstream_oracle() -> None:
    construction_only = SolverConfig.from_toml("""
environment_mode = "reproducible"
random_seed = 1

[termination]
seconds_spent_limit = 30
unimproved_seconds_spent_limit = 5

[[phases]]
type = "construction_heuristic"
construction_heuristic_type = "cheapest_insertion"
construction_obligation = "assign_when_candidate_exists"
entity_class = "Shift"
variable_name = "employee_idx"
""")

    plan = Solver.solve(demo_plan(), construction_only)

    assert_hard_feasible(plan)
    assert plan.score == {
        "family": "hard_soft_decimal",
        "levels": [0, 15_199_994],
    }


def test_solverforge_hospital_config_keeps_upstream_termination() -> None:
    config = HOSPITAL_SOLVER_CONFIG.to_dict()

    assert config["environment_mode"] == "reproducible"
    assert config["random_seed"] == 1
    assert config["termination"]["seconds_spent_limit"] == 30
    assert config["termination"]["unimproved_seconds_spent_limit"] == 5
    assert [phase["type"] for phase in config["phases"]] == [
        "construction_heuristic",
        "local_search",
    ]
    assert config["phases"][1]["acceptor"] == {
        "type": "late_acceptance",
        "late_acceptance_size": 400,
    }
    assert config["phases"][1]["forager"] == {"type": "first_best_score_improving"}
    assert [
        selector["type"]
        for selector in config["phases"][1]["move_selector"]["selectors"]
    ] == [
        "nearby_change_move_selector",
        "nearby_swap_move_selector",
    ]


def test_solverforge_hospital_prunes_statically_ineligible_employee_candidates() -> (
    None
):
    plan = demo_plan()
    field = build_schema(plan)["entities"][0]["fields"][0]

    assert field["candidate_values"] is eligible_employee_candidates

    for shift in plan.shifts:
        candidates = eligible_employee_candidates(shift)
        assert candidates == shift.employee_nearby_candidates
        assert candidates
        assert all(shift.employee_has_skill[candidate] for candidate in candidates)
        assert all(
            shift.employee_unavailable_minutes[candidate] == 0
            for candidate in candidates
        )


def test_solverforge_hospital_python_model_uses_canonical_large_payload() -> None:
    payload = fixture_payload()
    plan = demo_plan()

    assert len(payload["employees"]) == 50
    assert len(payload["shifts"]) == 688
    assert payload["score"] is None
    assert all(shift["employeeIdx"] is None for shift in payload["shifts"])
    assert plan_to_payload(plan) == payload


def test_solverforge_hospital_python_model_has_nonzero_initial_penalty() -> None:
    plan = demo_plan()
    Solver.analyze(plan)

    assert plan.score["family"] == "hard_soft_decimal"
    assert plan.score["levels"][0] < 0


def test_solverforge_hospital_rejects_negative_employee_index_payload() -> None:
    payload = deepcopy(fixture_payload())
    payload["shifts"][0]["employeeIdx"] = -1

    with pytest.raises(ValueError, match="employee_idx -1 is outside"):
        plan_from_payload(payload)


def test_solverforge_hospital_file_tree_matches_rust_ownership_shape() -> None:
    expected = {
        "src/api/dto.py",
        "src/api/mod.py",
        "src/api/routes.py",
        "src/api/sse.py",
        "src/constraints/assigned_shift.py",
        "src/constraints/balance_assignments.py",
        "src/constraints/desired_day.py",
        "src/constraints/minimum_rest.py",
        "src/constraints/mod.py",
        "src/constraints/one_shift_per_day.py",
        "src/constraints/overlapping_shift.py",
        "src/constraints/required_skill.py",
        "src/constraints/unavailable_employee.py",
        "src/constraints/undesired_day.py",
        "src/data/data_seed/availability.py",
        "src/data/data_seed/cohorts.py",
        "src/data/data_seed/coverage.py",
        "src/data/data_seed/demand.py",
        "src/data/data_seed/employees.py",
        "src/data/data_seed/entrypoints.py",
        "src/data/data_seed/large.py",
        "src/data/data_seed/preferences.py",
        "src/data/data_seed/shifts.py",
        "src/data/data_seed/skills.py",
        "src/data/data_seed/time_utils.py",
        "src/data/data_seed/validation.py",
        "src/data/data_seed/vocabulary.py",
        "src/data/data_seed/witness.py",
        "src/data/mod.py",
        "src/domain/care_hub.py",
        "src/domain/employee.py",
        "src/domain/mod.py",
        "src/domain/plan.py",
        "src/lib.py",
        "src/main.py",
        "src/solver/mod.py",
        "src/solver/service/mod.py",
        "src/solver/service/payload.py",
    }

    missing = [
        path
        for path in sorted(expected)
        if not (HOSPITAL_EXAMPLE_ROOT / path).is_file()
    ]

    assert missing == []
