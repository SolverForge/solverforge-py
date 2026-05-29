from __future__ import annotations

from typing import Any

from .large import fixture_payload


def required_skill_counts(payload: dict[str, Any] | None = None) -> dict[str, int]:
    source = payload or fixture_payload()
    counts: dict[str, int] = {}
    for shift in source.get("shifts", []):
        skill = str(shift.get("requiredSkill") or "")
        counts[skill] = counts.get(skill, 0) + 1
    return counts
