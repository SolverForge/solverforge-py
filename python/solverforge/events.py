from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class SolverEvent:
    job_id: int
    event_sequence: int
    lifecycle_state: str
    snapshot_revision: int | None = None
    error: str | None = None

