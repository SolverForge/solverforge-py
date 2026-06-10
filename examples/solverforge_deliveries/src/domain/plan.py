from __future__ import annotations

from typing import Any

from solverforge import HardSoftScore, planning_solution, shadow_variable_updates

from ..constraints import delivery_constraints
from .delivery import Delivery
from .metrics import build_preview, refresh_vehicle_route_shadows
from .vehicle import Vehicle


@planning_solution(
    score=HardSoftScore,
    constraints=delivery_constraints,
    shadow_updates=shadow_variable_updates(
        list_owner="vehicles",
        post_update_listener=refresh_vehicle_route_shadows,
    ),
)
class DeliveryPlan:
    deliveries: list[Delivery]
    vehicles: list[Vehicle]

    def __init__(
        self,
        *,
        name: str,
        deliveries: list[Delivery],
        vehicles: list[Vehicle],
        routing_mode: str = "straight_line",
        view_state: dict[str, Any] | None = None,
    ) -> None:
        self.name = name
        self.routing_mode = routing_mode
        self.view_state = view_state or {}
        self.deliveries = deliveries
        self.vehicles = vehicles
        self.delivery_indices = list(range(len(deliveries)))
        self.score = None
        self.normalize()

    def normalize(self) -> None:
        old_to_new = {
            delivery.id: index for index, delivery in enumerate(self.deliveries)
        }
        for index, delivery in enumerate(self.deliveries):
            delivery.id = index
        for index, vehicle in enumerate(self.vehicles):
            vehicle.id = index
            vehicle.delivery_order = [
                old_to_new[delivery_id]
                for delivery_id in vehicle.delivery_order
                if delivery_id in old_to_new
            ]
        self.delivery_indices = list(range(len(self.deliveries)))
        self.refresh_route_shadows()

    def refresh_route_shadows(self) -> None:
        assigned_counts = [0 for _ in self.deliveries]
        for vehicle in self.vehicles:
            for delivery_id in vehicle.delivery_order:
                if 0 <= delivery_id < len(assigned_counts):
                    assigned_counts[delivery_id] += 1
        for delivery, count in zip(self.deliveries, assigned_counts, strict=True):
            delivery.assigned_vehicle_count = count
        for index, vehicle in enumerate(self.vehicles):
            updates = refresh_vehicle_route_shadows(self, index)
            for name, value in updates.items():
                setattr(vehicle, name, value)

    def remove_delivery_assignments(self, delivery_id: int) -> None:
        for vehicle in self.vehicles:
            vehicle.delivery_order = [
                assigned
                for assigned in vehicle.delivery_order
                if assigned != delivery_id
            ]

    def refreshed_for_transport(self) -> DeliveryPlan:
        self.refresh_route_shadows()
        self.view_state = dict(self.view_state or {})
        self.view_state["preview"] = build_preview(self)
        return self
