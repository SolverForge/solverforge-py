from __future__ import annotations

from typing import Any

from ...domain import Employee, HospitalPlan
from .employees import employee_from_payload
from .large import fixture_payload
from .shifts import shift_from_payload
from .validation import validate_fixture_payload


def plan_from_payload(payload: dict[str, Any]) -> HospitalPlan:
    validate_fixture_payload(payload)
    employees = [
        employee_from_payload(item, index)
        for index, item in enumerate(payload.get("employees", []))
    ]
    shifts = [
        shift_from_payload(item, index, employees)
        for index, item in enumerate(payload.get("shifts", []))
    ]
    plan = HospitalPlan(employees, shifts)
    plan.score = payload.get("score")
    return plan


def demo_employees() -> list[Employee]:
    return plan_from_payload(fixture_payload()).employees


def demo_plan() -> HospitalPlan:
    return plan_from_payload(fixture_payload())
