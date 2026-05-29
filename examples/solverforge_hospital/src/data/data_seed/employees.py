from __future__ import annotations

from typing import Any

from ...domain import CareHub, Employee


def employee_from_payload(payload: dict[str, Any], index: int) -> Employee:
    return Employee(
        index=index,
        id=str(payload.get("id") or f"employee-{index}"),
        name=str(payload.get("name") or f"Employee {index + 1}"),
        home_hub=CareHub(str(payload.get("homeHub") or CareHub.UNKNOWN.value)),
        skills=tuple(str(skill) for skill in payload.get("skills", [])),
        unavailable_dates=tuple(str(value) for value in payload.get("unavailableDates", [])),
        undesired_dates=tuple(str(value) for value in payload.get("undesiredDates", [])),
        desired_dates=tuple(str(value) for value in payload.get("desiredDates", [])),
    )
