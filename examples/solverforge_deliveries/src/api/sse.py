from __future__ import annotations

import asyncio
import json
from collections.abc import AsyncIterator
from typing import Any

from fastapi import Request

from ..solver.service import JobRecord


async def sse_stream(
    request: Request,
    record: JobRecord,
    bootstrap: dict[str, Any],
) -> AsyncIterator[str]:
    last_sequence = int(bootstrap.get("eventSequence") or 0)
    yield sse_event(bootstrap)

    while not await request.is_disconnected():
        with record.condition:
            ready = [
                event
                for event in record.events
                if int(event.get("eventSequence") or 0) > last_sequence
            ]
            deleted = record.deleted
        if deleted:
            return
        if not ready:
            yield ": keep-alive\n\n"
            await asyncio.sleep(0.25)
            continue

        for event in ready:
            last_sequence = max(last_sequence, int(event.get("eventSequence") or 0))
            yield sse_event(event)


def sse_event(payload: dict[str, Any]) -> str:
    return f"data: {json.dumps(payload)}\n\n"
