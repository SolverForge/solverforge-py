from __future__ import annotations

from solverforge import planning_entity, planning_id, planning_list_variable

from .metrics import route_depot, route_distance, route_feasible, route_metric_class


@planning_entity
class Vehicle:
    id = planning_id()
    delivery_order = planning_list_variable(
        element_collection="delivery_indices",
        route_depot=route_depot,
        route_metric_class=route_metric_class,
        route_distance=route_distance,
        route_feasible=route_feasible,
    )

    def __init__(
        self,
        *,
        id: int,
        name: str,
        capacity: int,
        home_lat: float,
        home_lng: float,
        departure_time: int,
        delivery_order: list[int] | None = None,
    ) -> None:
        self.id = id
        self.name = name
        self.capacity = capacity
        self.home_lat = home_lat
        self.home_lng = home_lng
        self.departure_time = departure_time
        self.delivery_order = list(delivery_order or [])
        self.route_total_demand = 0
        self.route_capacity_overage = 0
        self.route_total_travel_seconds = 0
        self.route_time_window_violation_seconds = 0
        self.route_unreachable_legs = 0
