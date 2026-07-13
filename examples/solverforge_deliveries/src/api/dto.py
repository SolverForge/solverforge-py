from __future__ import annotations

from typing import Any

from solverforge import Solver

from ..constraints import CONSTRAINTS
from ..data.data_seed import plan_from_payload
from ..domain import (
    Delivery,
    DeliveryPlan,
    Vehicle,
    build_preview,
    evaluate_plan_components,
)
from ..domain.metrics import (
    CAPACITY_HARD_WEIGHT,
    UNASSIGNED_DELIVERY_HARD_PENALTY,
    hard_soft_string,
)


def plan_to_payload(plan: DeliveryPlan) -> dict[str, Any]:
    plan.refreshed_for_transport()
    return {
        "name": plan.name,
        "routingMode": plan.routing_mode,
        "viewState": dict(plan.view_state),
        "deliveries": [delivery_to_payload(delivery) for delivery in plan.deliveries],
        "vehicles": [vehicle_to_payload(vehicle) for vehicle in plan.vehicles],
        "score": score_to_string(plan.score),
    }


def delivery_to_payload(delivery: Delivery) -> dict[str, Any]:
    return {
        "id": delivery.id,
        "label": delivery.label,
        "kind": delivery.kind,
        "lat": delivery.lat,
        "lng": delivery.lng,
        "demand": delivery.demand,
        "minStartTime": delivery.min_start_time,
        "maxEndTime": delivery.max_end_time,
        "serviceDuration": delivery.service_duration,
    }


def vehicle_to_payload(vehicle: Vehicle) -> dict[str, Any]:
    return {
        "id": vehicle.id,
        "name": vehicle.name,
        "capacity": vehicle.capacity,
        "homeLat": vehicle.home_lat,
        "homeLng": vehicle.home_lng,
        "departureTime": vehicle.departure_time,
        "deliveryOrder": list(vehicle.delivery_order),
        "routeTotalDemand": vehicle.route_total_demand,
        "routeCapacityOverage": vehicle.route_capacity_overage,
        "routeTotalTravelSeconds": vehicle.route_total_travel_seconds,
        "routeTimeWindowViolationSeconds": vehicle.route_time_window_violation_seconds,
        "routeUnreachableLegs": vehicle.route_unreachable_legs,
    }


def payload_to_plan(payload: dict[str, Any]) -> DeliveryPlan:
    return plan_from_payload(payload)


def score_to_string(score: object) -> str | None:
    if isinstance(score, str):
        return score
    if not isinstance(score, dict):
        return None
    levels = score.get("levels")
    if not isinstance(levels, list) or len(levels) < 2:
        return None
    return hard_soft_string(int(levels[0]), int(levels[-1]))


def telemetry(payload: object | None = None) -> dict[str, Any]:
    if not isinstance(payload, dict):
        payload = {}
    phase = payload.get("phase")
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
        "phase": (
            {
                "phaseIndex": int(phase.get("phase_index", 0)),
                "phaseType": str(phase.get("phase_type", "")),
                "elapsedMs": int(phase.get("elapsed_ms", 0)),
                "stepCount": int(phase.get("step_count", 0)),
                "movesGenerated": int(phase.get("moves_generated", 0)),
                "movesEvaluated": int(phase.get("moves_evaluated", 0)),
                "movesAccepted": int(phase.get("moves_accepted", 0)),
                "movesApplied": int(phase.get("moves_applied", 0)),
                "scoreCalculations": int(phase.get("score_calculations", 0)),
                "generationMs": int(phase.get("generation_ms", 0)),
                "evaluationMs": int(phase.get("evaluation_ms", 0)),
            }
            if isinstance(phase, dict)
            else None
        ),
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
    plan: DeliveryPlan,
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
    plan: DeliveryPlan,
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


def analyze_plan(plan: DeliveryPlan) -> dict[str, Any]:
    Solver.analyze(plan)
    rows = constraint_analysis(plan)
    constraints = []
    for constraint in CONSTRAINTS:
        row = rows[constraint["name"]]
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


def constraint_analysis(plan: DeliveryPlan) -> dict[str, dict[str, object]]:
    plan.refresh_route_shadows()
    preview = build_preview(plan)
    components = evaluate_plan_components(plan)
    unassigned_count = components["unassignedCount"]
    rows = {
        "All Deliveries Assigned": {
            "weight": f"{UNASSIGNED_DELIVERY_HARD_PENALTY}hard/0soft",
            "matchCount": unassigned_count,
            "score": hard_soft_string(
                -(unassigned_count * UNASSIGNED_DELIVERY_HARD_PENALTY), 0
            ),
        },
        "Vehicle Capacity": {
            "weight": f"{CAPACITY_HARD_WEIGHT}hard/0soft",
            "matchCount": sum(
                1 for vehicle in preview["vehicles"] if vehicle["capacityOverage"] > 0
            ),
            "score": hard_soft_string(
                -(components["capacityOverage"] * CAPACITY_HARD_WEIGHT), 0
            ),
        },
        "Delivery Time Windows": {
            "weight": "1hard/0soft",
            "matchCount": sum(
                1 for vehicle in preview["vehicles"] if vehicle["totalLateSeconds"] > 0
            ),
            "score": hard_soft_string(-components["lateSeconds"], 0),
        },
        "Total Travel Time": {
            "weight": "0hard/1soft",
            "matchCount": len(plan.vehicles),
            "score": hard_soft_string(0, -components["travelSeconds"]),
        },
    }
    return rows
