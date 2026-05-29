from __future__ import annotations

from typing import Any

from .large import fixture_payload


def employee_preferences(payload: dict[str, Any] | None = None) -> list[dict[str, Any]]:
    source = payload or fixture_payload()
    return [
        {
            "id": employee.get("id"),
            "undesiredDates": list(employee.get("undesiredDates", [])),
            "desiredDates": list(employee.get("desiredDates", [])),
        }
        for employee in source.get("employees", [])
    ]
