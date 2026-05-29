from __future__ import annotations

from typing import Any

from ...domain import CareHub, Employee, Shift, care_hub_from_location


def shift_from_payload(payload: dict[str, Any], index: int, employees: list[Employee]) -> Shift:
    location = str(payload.get("location") or "")
    return Shift(
        id=str(payload.get("id") or index),
        index=index,
        start=str(payload["start"]),
        end=str(payload["end"]),
        location=location,
        care_hub=CareHub(str(payload.get("careHub") or care_hub_from_location(location).value)),
        required_skill=str(payload.get("requiredSkill") or ""),
        employees=employees,
        employee_idx=payload.get("employeeIdx"),
    )
