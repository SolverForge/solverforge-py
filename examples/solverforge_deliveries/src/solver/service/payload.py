from __future__ import annotations

from typing import Any

from ...api.dto import plan_to_payload, score_to_string, telemetry, terminal_reason
from ...domain import DeliveryPlan


def status_event_payload(
    record: Any,
    event_type: str,
    status: dict[str, object],
    *,
    solution: DeliveryPlan | None = None,
) -> dict[str, Any]:
    current_score = _solution_score(solution) or score_to_string(
        status.get("current_score")
    )
    best_score = score_to_string(status.get("best_score")) or current_score
    return {
        "id": record.id,
        "jobId": record.id,
        "eventType": event_type,
        "eventSequence": status.get("event_sequence"),
        "lifecycleState": status.get("lifecycle_state"),
        "terminalReason": terminal_reason(status.get("terminal_reason")),
        "telemetry": telemetry(status.get("telemetry")),
        "currentScore": current_score,
        "bestScore": best_score,
        "snapshotRevision": status.get("latest_snapshot_revision"),
        "solution": plan_to_payload(solution) if solution is not None else None,
        "error": None,
    }


def event_payload_from_native(
    record: Any,
    event_type: str,
    native_event: dict[str, object],
    *,
    solution: DeliveryPlan | None = None,
) -> dict[str, Any]:
    current_score = _solution_score(solution) or score_to_string(
        native_event.get("current_score")
    )
    best_score = score_to_string(native_event.get("best_score")) or current_score
    return {
        "id": record.id,
        "jobId": record.id,
        "eventType": event_type,
        "eventSequence": native_event.get("event_sequence"),
        "lifecycleState": native_event.get("lifecycle_state"),
        "terminalReason": terminal_reason(native_event.get("terminal_reason")),
        "telemetry": telemetry(native_event.get("telemetry")),
        "currentScore": current_score,
        "bestScore": best_score,
        "snapshotRevision": native_event.get("snapshot_revision"),
        "solution": plan_to_payload(solution) if solution is not None else None,
        "error": native_event.get("error"),
    }


def bootstrap_event_type(state: str) -> str:
    if state == "PAUSE_REQUESTED":
        return "pause_requested"
    if state == "PAUSED":
        return "paused"
    if state == "COMPLETED":
        return "completed"
    if state == "CANCELLED":
        return "cancelled"
    if state == "FAILED":
        return "failed"
    return "progress"


def bootstrap_snapshot_event_type(state: str) -> str:
    if state == "SOLVING":
        return "best_solution"
    return bootstrap_event_type(state)


def _solution_score(solution: DeliveryPlan | None) -> str | None:
    if solution is None:
        return None
    return score_to_string(solution.score)
