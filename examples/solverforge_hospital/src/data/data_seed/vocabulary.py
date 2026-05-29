from __future__ import annotations

from typing import Any

from .large import fixture_payload


def location_vocabulary(payload: dict[str, Any] | None = None) -> list[str]:
    source = payload or fixture_payload()
    return sorted({str(shift.get("location") or "") for shift in source.get("shifts", [])})
