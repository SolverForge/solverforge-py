from __future__ import annotations

from typing import Any

from .large import fixture_payload


def assigned_employee_indices(payload: dict[str, Any] | None = None) -> list[int | None]:
    source = payload or fixture_payload()
    return [shift.get("employeeIdx") for shift in source.get("shifts", [])]
