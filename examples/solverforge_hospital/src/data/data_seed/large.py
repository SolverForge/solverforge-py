from __future__ import annotations

import json
from pathlib import Path
from typing import Any

DATA_FILE = Path(__file__).with_name("LARGE.json")


def fixture_payload() -> dict[str, Any]:
    with DATA_FILE.open(encoding="utf-8") as handle:
        payload = json.load(handle)
    if not isinstance(payload, dict):
        msg = "hospital fixture must be a JSON object"
        raise TypeError(msg)
    return payload
