from __future__ import annotations

from pathlib import Path
from typing import cast

from solverforge import Solver, SolverConfig

from .data.data_seed import demo_employees, demo_plan, fixture_payload, plan_from_payload
from .domain import (
    CareHub,
    Employee,
    HospitalPlan,
    Shift,
    balance_score,
    gap_minutes,
    overlaps,
    same_day,
)
from .constraints import hospital_constraints

ROOT_DIR = Path(__file__).resolve().parents[1]
HOSPITAL_SOLVER_CONFIG = SolverConfig.load(ROOT_DIR / "solver.toml")


def solve_demo() -> HospitalPlan:
    return cast(HospitalPlan, Solver.solve(demo_plan(), HOSPITAL_SOLVER_CONFIG))


def assignment_summary(plan: HospitalPlan) -> list[tuple[str, str | None]]:
    return [
        (
            shift.id,
            None if shift.employee_idx is None else plan.employees[shift.employee_idx].name,
        )
        for shift in plan.shifts
    ]


__all__ = [
    "CareHub",
    "Employee",
    "HOSPITAL_SOLVER_CONFIG",
    "HospitalPlan",
    "Shift",
    "assignment_summary",
    "balance_score",
    "demo_employees",
    "demo_plan",
    "fixture_payload",
    "gap_minutes",
    "hospital_constraints",
    "overlaps",
    "plan_from_payload",
    "same_day",
    "solve_demo",
]
