from __future__ import annotations

from typing import Any, cast

from solverforge import Solver

from ..constraints import CONSTRAINTS
from ..data.data_seed import plan_from_payload
from ..domain import (
    SCORE_SCALE,
    STRUCTURAL_MINUTE_HARD_UNITS,
    Employee,
    HospitalPlan,
    Shift,
    balance_score,
    gap_minutes,
    lacks_required_skill,
    overlap_minutes,
    overlaps,
    same_day,
    unavailable_minutes,
)


def plan_to_payload(plan: HospitalPlan) -> dict[str, Any]:
    return {
        "employees": [employee_to_payload(employee) for employee in plan.employees],
        "shifts": [shift_to_payload(shift) for shift in plan.shifts],
        "score": score_to_string(plan.score),
    }


def employee_to_payload(employee: Employee) -> dict[str, Any]:
    return {
        "id": employee.id,
        "name": employee.name,
        "homeHub": employee.home_hub.value,
        "skills": list(employee.skills),
        "unavailableDates": list(employee.unavailable_dates),
        "undesiredDates": list(employee.undesired_dates),
        "desiredDates": list(employee.desired_dates),
    }


def shift_to_payload(shift: Shift) -> dict[str, Any]:
    return {
        "id": shift.id,
        "start": shift.start,
        "end": shift.end,
        "location": shift.location,
        "careHub": shift.care_hub.value,
        "requiredSkill": shift.required_skill,
        "employeeIdx": shift.employee_idx,
    }


def payload_to_plan(payload: dict[str, Any]) -> HospitalPlan:
    return plan_from_payload(payload)


def score_to_string(score: object) -> str | None:
    if isinstance(score, str):
        return score
    if not isinstance(score, dict):
        return None
    levels = score.get("levels")
    if not isinstance(levels, list) or len(levels) < 2:
        return None
    return f"{format_decimal_score_part(int(levels[0]))}hard/{format_decimal_score_part(int(levels[-1]))}soft"


def format_decimal_score_part(scaled: int) -> str:
    if scaled % 100_000 == 0:
        return str(scaled // 100_000)
    value = scaled / 100_000
    return f"{value:.6f}".rstrip("0").rstrip(".")


def telemetry(payload: object | None = None) -> dict[str, Any]:
    if not isinstance(payload, dict):
        payload = {}
    return {
        "elapsedMs": int(payload.get("elapsed_ms", 0)),
        "stepCount": int(payload.get("step_count", 0)),
        "movesGenerated": int(payload.get("moves_generated", 0)),
        "movesEvaluated": int(payload.get("moves_evaluated", 0)),
        "movesAccepted": int(payload.get("moves_accepted", 0)),
        "scoreCalculations": int(payload.get("score_calculations", 0)),
        "generationMs": int(payload.get("generation_ms", 0)),
        "evaluationMs": int(payload.get("evaluation_ms", 0)),
        "movesPerSecond": int(payload.get("moves_per_second", 0)),
        "acceptanceRate": float(payload.get("acceptance_rate", 0.0)),
    }


def terminal_reason(value: object) -> str | None:
    if value is None:
        return None
    return str(value).lower()


def status_payload(record: Any, status: dict[str, object]) -> dict[str, Any]:
    return {
        "id": record.id,
        "jobId": record.id,
        "lifecycleState": status.get("lifecycle_state"),
        "terminalReason": terminal_reason(status.get("terminal_reason")),
        "checkpointAvailable": bool(status.get("checkpoint_available")),
        "eventSequence": status.get("event_sequence"),
        "snapshotRevision": status.get("latest_snapshot_revision"),
        "currentScore": score_to_string(status.get("current_score")),
        "bestScore": score_to_string(status.get("best_score")),
        "telemetry": telemetry(status.get("telemetry")),
    }


def snapshot_payload(
    record: Any,
    plan: HospitalPlan,
    status: dict[str, object],
    revision: int | None,
) -> dict[str, Any]:
    resolved_revision = (
        revision if revision is not None else status.get("latest_snapshot_revision")
    )
    snapshot_score = score_to_string(plan.score)
    return {
        "id": record.id,
        "jobId": record.id,
        "snapshotRevision": resolved_revision,
        "lifecycleState": status.get("lifecycle_state"),
        "terminalReason": terminal_reason(status.get("terminal_reason")),
        "currentScore": snapshot_score,
        "bestScore": score_to_string(status.get("best_score")) or snapshot_score,
        "telemetry": telemetry(status.get("telemetry")),
        "solution": plan_to_payload(plan),
    }


def analysis_payload(
    record: Any,
    plan: HospitalPlan,
    status: dict[str, object],
    revision: int | None,
) -> dict[str, Any]:
    analysis = analyze_plan(plan)
    resolved_revision = (
        revision if revision is not None else status.get("latest_snapshot_revision")
    )
    return {
        "id": record.id,
        "jobId": record.id,
        "snapshotRevision": resolved_revision,
        "lifecycleState": status.get("lifecycle_state"),
        "terminalReason": terminal_reason(status.get("terminal_reason")),
        "analysis": analysis,
    }


def analyze_plan(plan: HospitalPlan) -> dict[str, Any]:
    Solver.analyze(plan)
    analysis_by_name = constraint_analysis(plan)
    constraints = []
    for constraint in CONSTRAINTS:
        row = analysis_by_name[constraint["name"]]
        constraints.append(
            {
                "name": constraint["name"],
                "type": constraint["type"],
                "weight": row["weight"],
                "score": row["score"],
                "matchCount": row["matchCount"],
            }
        )
    return {"score": score_to_string(plan.score), "constraints": constraints}


def constraint_analysis(plan: HospitalPlan) -> dict[str, dict[str, object]]:
    rows = {
        "Assigned shift": analysis_row("1hard/0soft"),
        "Required skill": analysis_row("10hard/0soft"),
        "Overlapping shift": analysis_row("0hard/0soft"),
        "At least 10 hours between 2 shifts": analysis_row("0hard/0soft"),
        "One shift per day": analysis_row("20hard/0soft"),
        "Unavailable employee": analysis_row("0hard/0soft"),
        "Undesired day for employee": analysis_row("0hard/1soft"),
        "Desired day for employee": analysis_row("0hard/1soft"),
        "Balance employee assignments": analysis_row("0hard/0.00001soft"),
    }
    for shift in plan.shifts:
        if shift.employee_idx is None:
            add_match(rows["Assigned shift"], hard=-SCORE_SCALE)
            continue
        if lacks_required_skill(shift):
            add_match(rows["Required skill"], hard=-(10 * SCORE_SCALE))
        unavailable = unavailable_minutes(shift)
        if unavailable > 0:
            add_match(
                rows["Unavailable employee"],
                hard=-(unavailable * STRUCTURAL_MINUTE_HARD_UNITS * SCORE_SCALE),
            )
        undesired_count = shift.employee_undesired_day_count[shift.employee_idx]
        if undesired_count > 0:
            add_match(
                rows["Undesired day for employee"],
                soft=-(undesired_count * SCORE_SCALE),
            )
        desired_count = shift.employee_desired_day_count[shift.employee_idx]
        if desired_count > 0:
            add_match(
                rows["Desired day for employee"], soft=desired_count * SCORE_SCALE
            )

    for left in plan.shifts:
        for right in plan.shifts:
            if left.index >= right.index or left.employee_idx != right.employee_idx:
                continue
            if overlaps(left, right):
                add_match(
                    rows["Overlapping shift"],
                    hard=-(
                        overlap_minutes(left, right)
                        * STRUCTURAL_MINUTE_HARD_UNITS
                        * SCORE_SCALE
                    ),
                )
            if same_day(left, right):
                add_match(rows["One shift per day"], hard=-(20 * SCORE_SCALE))
            gap = gap_minutes(left, right)
            if gap is not None and gap < 10 * 60:
                add_match(
                    rows["At least 10 hours between 2 shifts"],
                    hard=-(
                        (10 * 60 - gap) * STRUCTURAL_MINUTE_HARD_UNITS * SCORE_SCALE
                    ),
                )

    balance = balance_score(plan.shifts)
    balance_row = rows["Balance employee assignments"]
    balance_row["soft"] = -balance.soft_scaled
    balance_row["matchCount"] = balance_match_count(plan)
    for row in rows.values():
        row["score"] = hard_soft_string(row_int(row["hard"]), row_int(row["soft"]))
    return rows


def analysis_row(weight: str) -> dict[str, object]:
    return {
        "weight": weight,
        "hard": 0,
        "soft": 0,
        "matchCount": 0,
        "score": "0hard/0soft",
    }


def add_match(row: dict[str, object], *, hard: int = 0, soft: int = 0) -> None:
    row["hard"] = row_int(row["hard"]) + hard
    row["soft"] = row_int(row["soft"]) + soft
    row["matchCount"] = row_int(row["matchCount"]) + 1


def row_int(value: object) -> int:
    return int(cast(Any, value))


def hard_soft_string(hard: int, soft: int) -> str:
    return (
        f"{format_decimal_score_part(hard)}hard/{format_decimal_score_part(soft)}soft"
    )


def balance_match_count(plan: HospitalPlan) -> int:
    counts: dict[int, int] = {}
    for shift in plan.shifts:
        if shift.employee_idx is not None:
            counts[shift.employee_idx] = counts.get(shift.employee_idx, 0) + 1
    if not counts:
        return 0
    mean = sum(counts.values()) / len(counts)
    return sum(1 for count in counts.values() if abs(count - mean) > 0.5)
