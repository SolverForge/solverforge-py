from __future__ import annotations

from typing import Any

from .large import fixture_payload


def employees_by_home_hub(payload: dict[str, Any] | None = None) -> dict[str, list[str]]:
    source = payload or fixture_payload()
    cohorts: dict[str, list[str]] = {}
    for employee in source.get("employees", []):
        hub = str(employee.get("homeHub") or "unknown")
        cohorts.setdefault(hub, []).append(str(employee.get("id")))
    return cohorts
