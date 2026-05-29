from __future__ import annotations

from typing import Any


def validate_fixture_payload(payload: dict[str, Any]) -> None:
    if not isinstance(payload.get("employees"), list):
        msg = "hospital fixture must contain an employees list"
        raise TypeError(msg)
    if not isinstance(payload.get("shifts"), list):
        msg = "hospital fixture must contain a shifts list"
        raise TypeError(msg)
