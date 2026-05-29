from __future__ import annotations

import threading
import time
from dataclasses import dataclass, field
from http import HTTPStatus
from typing import Any, cast

from solverforge import SolverManager

from ...domain import HospitalPlan
from ...lib import HOSPITAL_SOLVER_CONFIG, assignment_summary, solve_demo
from .payload import (
    bootstrap_event_type,
    bootstrap_snapshot_event_type,
    event_payload_from_native,
    status_event_payload,
)

TERMINAL_STATES = {"COMPLETED", "CANCELLED", "FAILED"}
EVENT_NAME_BY_NATIVE = {
    "PROGRESS": "progress",
    "BEST_SOLUTION": "best_solution",
    "PAUSE_REQUESTED": "pause_requested",
    "PAUSED": "paused",
    "RESUMED": "resumed",
    "COMPLETED": "completed",
    "CANCELLED": "cancelled",
    "FAILED": "failed",
}


@dataclass
class JobRecord:
    id: str
    native_id: int
    condition: threading.Condition = field(default_factory=threading.Condition)
    events: list[dict[str, Any]] = field(default_factory=list)
    terminal: bool = False
    deleted: bool = False


class HospitalAppState:
    def __init__(self) -> None:
        self.manager = SolverManager(HOSPITAL_SOLVER_CONFIG)
        self._lock = threading.Lock()
        self._jobs: dict[str, JobRecord] = {}

    def create_job(self, plan: HospitalPlan) -> JobRecord:
        handle = self.manager.solve(plan)
        record = JobRecord(id=str(handle.job_id), native_id=handle.job_id)
        with self._lock:
            self._jobs[record.id] = record
        thread = threading.Thread(
            target=self._drain_job_events,
            args=(record,),
            daemon=True,
        )
        thread.start()
        return record

    def get_job(self, job_id: str) -> JobRecord | None:
        with self._lock:
            return self._jobs.get(job_id)

    def delete_job(self, job_id: str) -> None:
        record = self.require_job(job_id)
        self.manager.delete(record.native_id)
        with self._lock:
            self._jobs.pop(job_id, None)
        with record.condition:
            record.deleted = True
            record.condition.notify_all()

    def require_job(self, job_id: str) -> JobRecord:
        record = self.get_job(job_id)
        if record is None:
            msg = f"job {job_id} was not found"
            raise RuntimeError(msg)
        return record

    def status(self, record: JobRecord) -> dict[str, object]:
        return self.manager.get_status(record.native_id)

    def pause(self, record: JobRecord) -> None:
        self.manager.pause(record.native_id)

    def resume(self, record: JobRecord) -> None:
        self.manager.resume(record.native_id)

    def cancel(self, record: JobRecord) -> None:
        self.manager.cancel(record.native_id)

    def snapshot(self, record: JobRecord, revision: int | None = None) -> HospitalPlan:
        return cast(HospitalPlan, self.manager.snapshot(record.native_id, revision))

    def bootstrap_event(self, record: JobRecord) -> dict[str, Any]:
        status = self.status(record)
        revision = status.get("latest_snapshot_revision")
        state = str(status.get("lifecycle_state") or "SOLVING")
        if revision is None:
            return status_event_payload(
                record,
                bootstrap_event_type(state),
                status,
            )

        solution = self.snapshot(record, int(cast(Any, revision)))
        return status_event_payload(
            record,
            bootstrap_snapshot_event_type(state),
            status,
            solution=solution,
        )

    def _drain_job_events(self, record: JobRecord) -> None:
        while True:
            with record.condition:
                if record.deleted:
                    return
            try:
                native_events = self.manager.events(record.native_id)
            except Exception:
                return

            for native_event in native_events:
                event = self._event_payload(record, native_event)
                self._append_event(record, event)

            if record.terminal:
                return

            time.sleep(0.01)

    def _append_event(self, record: JobRecord, event: dict[str, Any]) -> None:
        with record.condition:
            duplicate = any(
                existing.get("eventSequence") == event.get("eventSequence")
                and existing.get("eventType") == event.get("eventType")
                for existing in record.events
            )
            if not duplicate:
                record.events.append(event)
            if event.get("lifecycleState") in TERMINAL_STATES:
                record.terminal = True
            record.condition.notify_all()

    def _event_payload(
        self,
        record: JobRecord,
        native_event: dict[str, object],
    ) -> dict[str, Any]:
        event_type = EVENT_NAME_BY_NATIVE.get(
            str(native_event.get("event_type")),
            str(native_event.get("event_type") or "").lower(),
        )
        solution = None
        revision = native_event.get("snapshot_revision")
        if event_type in {"best_solution", "completed"} and revision is not None:
            try:
                solution = self.snapshot(record, int(cast(Any, revision)))
            except Exception:
                solution = None

        return event_payload_from_native(
            record,
            event_type,
            native_event,
            solution=solution,
        )


def solve_summary() -> dict[str, Any]:
    solved = solve_demo()
    return {
        "score": str(solved.score),
        "assignments": [
            {"shift": shift_label, "employee": employee_name}
            for shift_label, employee_name in assignment_summary(solved)
        ],
    }


def status_from_manager_error(error: Exception) -> HTTPStatus:
    message = str(error)
    if "planning solution" in message or "entity collection" in message:
        return HTTPStatus.BAD_REQUEST
    if "no free job slots" in message:
        return HTTPStatus.SERVICE_UNAVAILABLE
    if "was not found" in message or "not found" in message:
        return HTTPStatus.NOT_FOUND
    if "cannot" in message or "NoSnapshotAvailable" in message or "no retained snapshots" in message:
        return HTTPStatus.CONFLICT
    return HTTPStatus.INTERNAL_SERVER_ERROR


__all__ = [
    "HospitalAppState",
    "JobRecord",
    "solve_summary",
    "status_from_manager_error",
]
