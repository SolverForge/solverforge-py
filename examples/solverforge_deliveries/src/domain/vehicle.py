from __future__ import annotations

from typing import Any
from weakref import ReferenceType, ref

from solverforge import (
    ListRouteHooks,
    ListSavingsHooks,
    RowField,
    SolutionCallback,
    planning_entity,
    planning_id,
    planning_list_variable,
)

from .metrics import route_feasible


@planning_entity
class Vehicle:
    id = planning_id()
    delivery_order = planning_list_variable(
        element_collection="delivery_indices",
        route=ListRouteHooks(
            depot=RowField("depot"),
            distance=RowField("distance_matrix"),
            feasible=SolutionCallback(route_feasible),
        ),
        savings=ListSavingsHooks(
            depot=RowField("depot"),
            metric_class=RowField("metric_class"),
            distance=RowField("distance_matrix"),
            feasible=SolutionCallback(route_feasible),
        ),
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
        self.depot = 0
        self.metric_class = 0
        self._distance_matrix: list[list[int]] = []
        self._distance_plan_ref: ReferenceType[Any] | None = None
        self.distance_matrix_signature: tuple[tuple[float, float], ...] | None = None
        self.delivery_order = list(delivery_order or [])
        self.route_total_demand = 0
        self.route_capacity_overage = 0
        self.route_total_travel_seconds = 0
        self.route_time_window_violation_seconds = 0
        self.route_unreachable_legs = 0

    @property
    def distance_matrix(self) -> list[list[int]]:
        self.refresh_distance_matrix()
        return self._distance_matrix

    @distance_matrix.setter
    def distance_matrix(self, value: list[list[int]]) -> None:
        self._distance_matrix = value

    def bind_distance_plan(self, plan: object) -> None:
        self._distance_plan_ref = ref(plan)

    def refresh_distance_matrix(self) -> None:
        if self._distance_plan_ref is None:
            return
        plan = self._distance_plan_ref()
        if plan is None:
            return
        signature = plan._vehicle_distance_signature(self)
        if not self._distance_matrix or self.distance_matrix_signature != signature:
            self._distance_matrix = plan._vehicle_distance_matrix(self)
            self.distance_matrix_signature = signature

    def __deepcopy__(self, memo: dict[int, object]) -> Vehicle:
        copied = type(self)(
            id=self.id,
            name=self.name,
            capacity=self.capacity,
            home_lat=self.home_lat,
            home_lng=self.home_lng,
            departure_time=self.departure_time,
            delivery_order=list(self.delivery_order),
        )
        memo[id(self)] = copied
        copied.depot = self.depot
        copied.metric_class = self.metric_class
        # The matrix is immutable for its coordinate signature and can be shared.
        copied.distance_matrix = self.distance_matrix
        copied.distance_matrix_signature = self.distance_matrix_signature
        copied.route_total_demand = self.route_total_demand
        copied.route_capacity_overage = self.route_capacity_overage
        copied.route_total_travel_seconds = self.route_total_travel_seconds
        copied.route_time_window_violation_seconds = (
            self.route_time_window_violation_seconds
        )
        copied.route_unreachable_legs = self.route_unreachable_legs
        return copied
