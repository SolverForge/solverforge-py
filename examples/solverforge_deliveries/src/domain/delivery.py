from __future__ import annotations

from solverforge import planning_id, problem_fact


@problem_fact
class Delivery:
    id = planning_id()

    def __init__(
        self,
        *,
        id: int,
        label: str,
        kind: str,
        lat: float,
        lng: float,
        demand: int,
        min_start_time: int,
        max_end_time: int,
        service_duration: int,
    ) -> None:
        self.id = id
        self.label = label
        self.kind = kind
        self.lat = lat
        self.lng = lng
        self.demand = demand
        self.min_start_time = min_start_time
        self.max_end_time = max_end_time
        self.service_duration = service_duration
        self.assigned_vehicle_count = 0
