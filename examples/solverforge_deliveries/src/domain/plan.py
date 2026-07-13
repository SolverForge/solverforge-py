from __future__ import annotations

from copy import deepcopy
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
            vehicle.depot = len(self.deliveries)
            vehicle.metric_class = index
            vehicle.bind_distance_plan(self)
            vehicle.refresh_distance_matrix()
            vehicle.delivery_order = [
                old_to_new[delivery_id]
                for delivery_id in vehicle.delivery_order
                if delivery_id in old_to_new
            ]
        self.delivery_indices = list(range(len(self.deliveries)))
        self.refresh_route_shadows()

    def __deepcopy__(self, memo: dict[int, object]) -> DeliveryPlan:
        copied = type(self).__new__(type(self))
        memo[id(self)] = copied
        copied.name = self.name
        copied.routing_mode = self.routing_mode
        copied.view_state = deepcopy(self.view_state, memo)
        copied.deliveries = deepcopy(self.deliveries, memo)
        copied.vehicles = deepcopy(self.vehicles, memo)
        copied.delivery_indices = list(self.delivery_indices)
        copied.score = deepcopy(self.score, memo)
        for vehicle in copied.vehicles:
            vehicle.bind_distance_plan(copied)
        return copied

    def _vehicle_distance_signature(
        self, vehicle: Vehicle
    ) -> tuple[tuple[float, float], ...]:
        return tuple(
            [(delivery.lat, delivery.lng) for delivery in self.deliveries]
            + [(vehicle.home_lat, vehicle.home_lng)]
        )

    def _vehicle_distance_matrix(self, vehicle: Vehicle) -> list[list[int]]:
        from .metrics import haversine_meters, meters_to_seconds

        coords = [(delivery.lat, delivery.lng) for delivery in self.deliveries]
        coords.append((vehicle.home_lat, vehicle.home_lng))
        matrix: list[list[int]] = []
        for from_coord in coords:
            row: list[int] = []
            for to_coord in coords:
                row.append(
                    meters_to_seconds(round(haversine_meters(*from_coord, *to_coord)))
                )
            matrix.append(row)
        return matrix

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
