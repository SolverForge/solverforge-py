from __future__ import annotations

from typing import Any

from .large import fixture_payload


def skill_vocabulary(payload: dict[str, Any] | None = None) -> list[str]:
    source = payload or fixture_payload()
    skills = {
        str(skill)
        for employee in source.get("employees", [])
        for skill in employee.get("skills", [])
    }
    return sorted(skills)
