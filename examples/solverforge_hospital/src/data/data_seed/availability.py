from __future__ import annotations

from typing import Any

from .large import fixture_payload


def employee_availability(payload: dict[str, Any] | None = None) -> list[dict[str, Any]]:
    source = payload or fixture_payload()
    return [
        {
            "id": employee.get("id"),
            "unavailableDates": list(employee.get("unavailableDates", [])),
        }
        for employee in source.get("employees", [])
    ]
