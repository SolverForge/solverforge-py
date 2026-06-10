from __future__ import annotations

import json
import random
from pathlib import Path
from typing import Any

from ...domain import Delivery, DeliveryPlan, Vehicle, route_distance, route_feasible

DATA_ROOT = Path(__file__).resolve().parent
LOCATIONS = json.loads((DATA_ROOT / "locations.json").read_text())
VEHICLE_NAMES = [
    "Alpha",
    "Bravo",
    "Charlie",
    "Delta",
    "Echo",
    "Foxtrot",
    "Golf",
    "Hotel",
    "India",
    "Juliet",
]
DEMO_LABELS = {
    "PHILADELPHIA": "Philadelphia",
    "HARTFORD": "Hartford",
    "FIRENZE": "Firenze",
}
DEMO_SEEDS = {
    "PHILADELPHIA": 0,
    "HARTFORD": 1,
    "FIRENZE": 2,
}
DEMO_CAPACITY = {
    "PHILADELPHIA": (36, 48),
    "HARTFORD": (24, 34),
    "FIRENZE": (38, 52),
}


def list_demo_data() -> list[str]:
    return ["PHILADELPHIA", "HARTFORD", "FIRENZE"]


def demo_plan(demo_id: str = "PHILADELPHIA") -> DeliveryPlan:
    normalized = demo_id.strip().upper()
    if normalized not in LOCATIONS:
        msg = f"unknown delivery demo data set {demo_id!r}"
        raise KeyError(msg)
    rng = random.Random(DEMO_SEEDS[normalized])
    min_capacity, max_capacity = DEMO_CAPACITY[normalized]
    data = LOCATIONS[normalized]
    vehicles = [
        Vehicle(
            id=index,
            name=VEHICLE_NAMES[index % len(VEHICLE_NAMES)],
            capacity=rng.randint(min_capacity, max_capacity),
            home_lat=float(depot["lat"]),
            home_lng=float(depot["lng"]),
            departure_time=6 * 3600,
        )
        for index, depot in enumerate(data["depots"])
    ]
    deliveries = [
        delivery_from_location(index, location, rng)
        for index, location in enumerate(data["visits"])
    ]
    plan = DeliveryPlan(
        name=DEMO_LABELS[normalized],
        deliveries=deliveries,
        vehicles=vehicles,
        routing_mode="road_network",
    )
    assign_initial_routes(plan)
    return plan


def delivery_from_location(
    index: int, location: dict[str, Any], rng: random.Random
) -> Delivery:
    kind, min_start, max_end, demand_range, service_range = customer_profile(
        str(location["customerType"])
    )
    return Delivery(
        id=index,
        label=str(location["name"]),
        kind=kind,
        lat=float(location["lat"]),
        lng=float(location["lng"]),
        demand=rng.randint(*demand_range),
        min_start_time=min_start,
        max_end_time=max_end,
        service_duration=rng.randint(*service_range),
    )


def customer_profile(
    kind: str,
) -> tuple[str, int, int, tuple[int, int], tuple[int, int]]:
    if kind == "residential":
        return ("residential", 17 * 3600, 20 * 3600, (1, 2), (5 * 60, 10 * 60))
    if kind == "business":
        return ("business", 9 * 3600, 17 * 3600, (3, 6), (15 * 60, 30 * 60))
    if kind == "restaurant":
        return ("restaurant", 6 * 3600, 10 * 3600, (5, 10), (20 * 60, 40 * 60))
    return ("other", 0, 24 * 3600, (1, 3), (5 * 60, 15 * 60))


def assign_initial_routes(plan: DeliveryPlan) -> None:
    ordered_deliveries = sorted(
        plan.deliveries,
        key=lambda delivery: (
            delivery.max_end_time,
            delivery.min_start_time,
            -delivery.demand,
            delivery.id,
        ),
    )
    for delivery in ordered_deliveries:
        best: tuple[int, int, int, int] | None = None
        for vehicle_index, vehicle in enumerate(plan.vehicles):
            for insert_index in range(len(vehicle.delivery_order) + 1):
                candidate = [
                    *vehicle.delivery_order[:insert_index],
                    delivery.id,
                    *vehicle.delivery_order[insert_index:],
                ]
                if not route_feasible(plan, vehicle_index, candidate):
                    continue
                total_travel = route_travel_seconds(plan, vehicle_index, candidate)
                key = (total_travel, len(candidate), vehicle_index, insert_index)
                if best is None or key < best:
                    best = key
        if best is None:
            vehicle_index, vehicle = min(
                enumerate(plan.vehicles),
                key=lambda item: (
                    depot_delivery_distance(item[1], delivery),
                    len(item[1].delivery_order),
                    item[0],
                ),
            )
            vehicle.delivery_order.append(delivery.id)
        else:
            _, _, vehicle_index, insert_index = best
            vehicle = plan.vehicles[vehicle_index]
            vehicle.delivery_order.insert(insert_index, delivery.id)
    plan.refresh_route_shadows()


def route_travel_seconds(
    plan: DeliveryPlan, vehicle_index: int, route: list[int]
) -> int:
    if not route:
        return 0
    depot = len(plan.deliveries)
    previous = depot
    total = 0
    for delivery_id in route:
        total += route_distance(plan, vehicle_index, previous, delivery_id)
        previous = delivery_id
    total += route_distance(plan, vehicle_index, previous, depot)
    return total


def depot_delivery_distance(vehicle: Vehicle, delivery: Delivery) -> float:
    lat_delta = float(vehicle.home_lat) - float(delivery.lat)
    lng_delta = float(vehicle.home_lng) - float(delivery.lng)
    return (lat_delta * lat_delta) + (lng_delta * lng_delta)


def plan_from_payload(payload: dict[str, Any]) -> DeliveryPlan:
    deliveries = [
        Delivery(
            id=int(delivery.get("id", index)),
            label=str(delivery.get("label", f"Delivery {index + 1}")),
            kind=str(delivery.get("kind", "other")),
            lat=float(delivery.get("lat", 0.0)),
            lng=float(delivery.get("lng", 0.0)),
            demand=int(delivery.get("demand", 1)),
            min_start_time=int(delivery.get("minStartTime", 0)),
            max_end_time=int(delivery.get("maxEndTime", 24 * 3600)),
            service_duration=int(delivery.get("serviceDuration", 0)),
        )
        for index, delivery in enumerate(payload.get("deliveries") or [])
    ]
    vehicles = [
        Vehicle(
            id=int(vehicle.get("id", index)),
            name=str(vehicle.get("name", f"Vehicle {index + 1}")),
            capacity=int(vehicle.get("capacity", 0)),
            home_lat=float(vehicle.get("homeLat", 0.0)),
            home_lng=float(vehicle.get("homeLng", 0.0)),
            departure_time=int(vehicle.get("departureTime", 0)),
            delivery_order=[
                int(delivery_id) for delivery_id in vehicle.get("deliveryOrder", [])
            ],
        )
        for index, vehicle in enumerate(payload.get("vehicles") or [])
    ]
    return DeliveryPlan(
        name=str(payload.get("name", "Delivery Plan")),
        deliveries=deliveries,
        vehicles=vehicles,
        routing_mode=str(payload.get("routingMode", "straight_line")),
        view_state=dict(payload.get("viewState") or {}),
    )


def fixture_payload(demo_id: str = "PHILADELPHIA") -> dict[str, Any]:
    from ...api.dto import plan_to_payload

    return plan_to_payload(demo_plan(demo_id))
