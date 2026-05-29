from __future__ import annotations

import time
from dataclasses import dataclass
from typing import Any

from . import _native
from .config import SolverConfig, _resolve_config
from .model import build_schema


TERMINAL_STATES = {"COMPLETED", "CANCELLED", "FAILED"}


@dataclass(frozen=True)
class JobHandle:
    job_id: int


class SolverManager:
    def __init__(self, config: SolverConfig | dict[str, Any] | None = None) -> None:
        self._native = _native.SolverManager(_resolve_config(config))

    def solve(self, solution: object) -> JobHandle:
        schema = build_schema(solution)
        return JobHandle(job_id=self._native.solve(solution, schema))

    def get_status(self, job_id: int) -> dict[str, object]:
        return self._native.get_status(job_id)

    def events(self, job_id: int) -> list[dict[str, object]]:
        return self._native.drain_events(job_id)

    def wait(self, job_id: int, timeout_seconds: float = 5.0) -> dict[str, object]:
        deadline = time.monotonic() + timeout_seconds
        while True:
            status = self.get_status(job_id)
            if status["lifecycle_state"] in TERMINAL_STATES:
                return status
            if time.monotonic() >= deadline:
                msg = f"job {job_id} did not finish within {timeout_seconds} seconds"
                raise TimeoutError(msg)
            time.sleep(0.01)

    def snapshot(self, job_id: int, snapshot_revision: int | None = None) -> object:
        return self._native.snapshot(job_id, snapshot_revision)

    def pause(self, job_id: int) -> None:
        self._native.pause(job_id)

    def resume(self, job_id: int) -> None:
        self._native.resume(job_id)

    def cancel(self, job_id: int) -> None:
        self._native.cancel(job_id)

    def delete(self, job_id: int) -> None:
        self._native.delete(job_id)
