from __future__ import annotations

from typing import Any

from .large import fixture_payload


def shifts(payload: dict[str, Any] | None = None) -> list[dict[str, Any]]:
    return list((payload or fixture_payload()).get("shifts", []))
