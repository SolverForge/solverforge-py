from __future__ import annotations

from typing import Any

from solverforge import ConstraintFactory, HardSoftScore, constraint_provider
from ..domain.metrics import CAPACITY_HARD_WEIGHT, UNASSIGNED_DELIVERY_HARD_PENALTY

CONSTRAINTS = [
    {"name": "All Deliveries Assigned", "type": "hard"},
    {"name": "Vehicle Capacity", "type": "hard"},
    {"name": "Delivery Time Windows", "type": "hard"},
    {"name": "Total Travel Time", "type": "soft"},
]


def capacity_penalty(vehicle: Any) -> HardSoftScore:
    return HardSoftScore.of_hard(
        int(vehicle.route_capacity_overage) * CAPACITY_HARD_WEIGHT
    )


def time_window_penalty(vehicle: Any) -> HardSoftScore:
    return HardSoftScore.of_hard(int(vehicle.route_time_window_violation_seconds))


def travel_penalty(vehicle: Any) -> HardSoftScore:
    return HardSoftScore.of_soft(int(vehicle.route_total_travel_seconds))


@constraint_provider
def delivery_constraints(factory: ConstraintFactory) -> list[object]:
    from ..domain.vehicle import Vehicle

    return [
        factory.for_each_unassigned_element(Vehicle, "delivery_order")
        .penalize(HardSoftScore.of_hard(UNASSIGNED_DELIVERY_HARD_PENALTY))
        .named("All Deliveries Assigned"),
        factory.for_each(Vehicle)
        .filter(lambda vehicle: vehicle.route_capacity_overage > 0)
        .penalize(capacity_penalty)
        .named("Vehicle Capacity"),
        factory.for_each(Vehicle)
        .filter(lambda vehicle: vehicle.route_time_window_violation_seconds > 0)
        .penalize(time_window_penalty)
        .named("Delivery Time Windows"),
        factory.for_each(Vehicle).penalize(travel_penalty).named("Total Travel Time"),
    ]
