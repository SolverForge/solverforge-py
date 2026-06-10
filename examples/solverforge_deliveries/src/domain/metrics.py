from __future__ import annotations

import math
from copy import deepcopy
from typing import Any

UNASSIGNED_DELIVERY_HARD_PENALTY = 1_000_000
CAPACITY_HARD_WEIGHT = 1_000_001
AVERAGE_SPEED_KMPH = 50


def route_depot(solution: Any, entity_index: int) -> int:
    del entity_index
    return len(solution.deliveries)


def route_metric_class(solution: Any, entity_index: int) -> int:
    del solution, entity_index
    return 0


def route_distance(
    solution: Any,
    entity_index: int,
    from_element: int,
    to_element: int,
) -> int:
    vehicle = solution.vehicles[entity_index]
    from_coord = route_coord(solution, vehicle, from_element)
    to_coord = route_coord(solution, vehicle, to_element)
    return meters_to_seconds(round(haversine_meters(*from_coord, *to_coord)))


def route_feasible(solution: Any, entity_index: int, route: list[int]) -> bool:
    vehicle = solution.vehicles[entity_index]
    total_demand = 0
    current_time = int(vehicle.departure_time)
    previous = route_depot(solution, entity_index)
    for delivery_id in route:
        if not 0 <= delivery_id < len(solution.deliveries):
            return False
        delivery = solution.deliveries[delivery_id]
        total_demand += int(delivery.demand)
        if total_demand > int(vehicle.capacity):
            return False
        current_time += route_distance(solution, entity_index, previous, delivery_id)
        service_start = max(current_time, int(delivery.min_start_time))
        current_time = service_start + int(delivery.service_duration)
        if current_time > int(delivery.max_end_time):
            return False
        previous = delivery_id
    return True


def refresh_vehicle_route_shadows(solution: Any, entity_index: int) -> dict[str, int]:
    vehicle = solution.vehicles[entity_index]
    metrics = vehicle_metrics(solution, vehicle)
    return {
        "route_total_demand": int(metrics["totalDemand"]),
        "route_capacity_overage": int(metrics["capacityOverage"]),
        "route_total_travel_seconds": int(metrics["totalTravelSeconds"]),
        "route_time_window_violation_seconds": int(metrics["totalLateSeconds"]),
        "route_unreachable_legs": 0,
    }


def build_preview(plan: Any) -> dict[str, Any]:
    assigned: set[int] = set()
    vehicles = []
    deliveries = [
        {
            "deliveryId": delivery.id,
            "label": delivery.label,
            "kind": delivery.kind,
            "demand": delivery.demand,
            "minStartTime": delivery.min_start_time,
            "maxEndTime": delivery.max_end_time,
            "serviceDuration": delivery.service_duration,
            "assignedVehicleId": None,
            "assignedVehicleName": None,
            "sequence": None,
            "arrivalTime": None,
            "serviceStartTime": None,
            "departureTime": None,
            "lateSeconds": None,
        }
        for delivery in plan.deliveries
    ]
    capacity_overage = 0
    late_seconds = 0
    travel_seconds = 0
    for vehicle in plan.vehicles:
        metrics = vehicle_metrics(plan, vehicle)
        vehicles.append(metrics)
        capacity_overage += int(metrics["capacityOverage"])
        late_seconds += int(metrics["totalLateSeconds"])
        travel_seconds += int(metrics["totalTravelSeconds"])
        for stop in metrics["stops"]:
            delivery_id = int(stop["deliveryId"])
            assigned.add(delivery_id)
            delivery = deliveries[delivery_id]
            delivery["assignedVehicleId"] = vehicle.id
            delivery["assignedVehicleName"] = vehicle.name
            delivery["sequence"] = stop["sequence"]
            delivery["arrivalTime"] = stop["arrivalTime"]
            delivery["serviceStartTime"] = stop["serviceStartTime"]
            delivery["departureTime"] = stop["departureTime"]
            delivery["lateSeconds"] = stop["lateSeconds"]
    unassigned = [
        delivery["deliveryId"]
        for delivery in deliveries
        if int(delivery["deliveryId"]) not in assigned
    ]
    return {
        "hardScore": -(
            len(unassigned) * UNASSIGNED_DELIVERY_HARD_PENALTY
            + capacity_overage * CAPACITY_HARD_WEIGHT
            + late_seconds
        ),
        "softScore": -travel_seconds,
        "unassignedDeliveryIds": unassigned,
        "vehicles": vehicles,
        "deliveries": deliveries,
    }


def vehicle_metrics(plan: Any, vehicle: Any) -> dict[str, Any]:
    stops = []
    total_demand = 0
    total_travel_seconds = 0
    total_wait_seconds = 0
    total_service_seconds = 0
    total_late_seconds = 0
    total_distance_meters = 0
    current_time = int(vehicle.departure_time)
    end_time = current_time
    previous: tuple[float, float] = (float(vehicle.home_lat), float(vehicle.home_lng))

    for sequence, delivery_id in enumerate(vehicle.delivery_order):
        if not 0 <= delivery_id < len(plan.deliveries):
            continue
        delivery = plan.deliveries[delivery_id]
        current = (float(delivery.lat), float(delivery.lng))
        meters = round(haversine_meters(*previous, *current))
        travel = meters_to_seconds(meters)
        arrival = current_time + travel
        service_start = max(arrival, int(delivery.min_start_time))
        wait_seconds = max(0, service_start - arrival)
        departure = service_start + int(delivery.service_duration)
        late_seconds = max(0, departure - int(delivery.max_end_time))

        total_demand += int(delivery.demand)
        total_distance_meters += meters
        total_travel_seconds += travel
        total_wait_seconds += wait_seconds
        total_service_seconds += int(delivery.service_duration)
        total_late_seconds += late_seconds
        current_time = departure
        end_time = departure
        previous = current
        stops.append(
            {
                "deliveryId": delivery_id,
                "label": delivery.label,
                "kind": delivery.kind,
                "sequence": sequence,
                "demand": delivery.demand,
                "minStartTime": delivery.min_start_time,
                "maxEndTime": delivery.max_end_time,
                "arrivalTime": arrival,
                "serviceStartTime": service_start,
                "departureTime": departure,
                "travelSecondsFromPrevious": travel,
                "waitSeconds": wait_seconds,
                "lateSeconds": late_seconds,
            }
        )

    if stops:
        depot = (float(vehicle.home_lat), float(vehicle.home_lng))
        return_meters = round(haversine_meters(*previous, *depot))
        return_travel = meters_to_seconds(return_meters)
        total_distance_meters += return_meters
        total_travel_seconds += return_travel
        end_time = current_time + return_travel

    return {
        "vehicleId": vehicle.id,
        "vehicleName": vehicle.name,
        "capacity": int(vehicle.capacity),
        "totalDemand": total_demand,
        "capacityOverage": max(0, total_demand - int(vehicle.capacity)),
        "stopCount": len(stops),
        "totalTravelSeconds": total_travel_seconds,
        "totalWaitSeconds": total_wait_seconds,
        "totalServiceSeconds": total_service_seconds,
        "totalLateSeconds": total_late_seconds,
        "totalDistanceMeters": total_distance_meters,
        "startTime": vehicle.departure_time,
        "endTime": end_time,
        "stops": stops,
    }


def evaluate_plan_components(plan: Any) -> dict[str, int]:
    assigned = {
        delivery_id
        for vehicle in plan.vehicles
        for delivery_id in vehicle.delivery_order
        if 0 <= delivery_id < len(plan.deliveries)
    }
    preview = build_preview(plan)
    return {
        "unassignedCount": len(plan.deliveries) - len(assigned),
        "capacityOverage": sum(
            int(vehicle["capacityOverage"]) for vehicle in preview["vehicles"]
        ),
        "lateSeconds": sum(
            int(vehicle["totalLateSeconds"]) for vehicle in preview["vehicles"]
        ),
        "travelSeconds": sum(
            int(vehicle["totalTravelSeconds"]) for vehicle in preview["vehicles"]
        ),
    }


def build_routes_snapshot(plan: Any) -> dict[str, Any]:
    vehicles = []
    for vehicle in plan.vehicles:
        metrics = vehicle_metrics(plan, vehicle)
        segments = route_segments(plan, vehicle)
        vehicles.append(
            {
                "vehicleId": vehicle.id,
                "vehicleName": vehicle.name,
                "totalTravelSeconds": metrics["totalTravelSeconds"],
                "totalDistanceMeters": metrics["totalDistanceMeters"],
                "totalDemand": metrics["totalDemand"],
                "totalLateSeconds": metrics["totalLateSeconds"],
                "stopCount": len(vehicle.delivery_order),
                "segments": segments,
            }
        )
    return {
        "routingMode": getattr(plan, "routing_mode", "straight_line"),
        "bounds": route_bounds(plan),
        "vehicles": vehicles,
    }


def route_segments(plan: Any, vehicle: Any) -> list[dict[str, Any]]:
    segments = []
    previous_coord = (float(vehicle.home_lat), float(vehicle.home_lng))
    previous_id: int | None = None
    for delivery_id in vehicle.delivery_order:
        if not 0 <= delivery_id < len(plan.deliveries):
            continue
        delivery = plan.deliveries[delivery_id]
        coord = (float(delivery.lat), float(delivery.lng))
        meters = round(haversine_meters(*previous_coord, *coord))
        segments.append(
            {
                "vehicleId": vehicle.id,
                "fromKind": "delivery" if previous_id is not None else "depot",
                "fromId": previous_id,
                "toKind": "delivery",
                "toId": delivery_id,
                "durationSeconds": meters_to_seconds(meters),
                "distanceMeters": meters,
                "encodedPolyline": encode_polyline([previous_coord, coord]),
            }
        )
        previous_coord = coord
        previous_id = delivery_id
    if previous_id is not None:
        depot = (float(vehicle.home_lat), float(vehicle.home_lng))
        meters = round(haversine_meters(*previous_coord, *depot))
        segments.append(
            {
                "vehicleId": vehicle.id,
                "fromKind": "delivery",
                "fromId": previous_id,
                "toKind": "depot",
                "toId": None,
                "durationSeconds": meters_to_seconds(meters),
                "distanceMeters": meters,
                "encodedPolyline": encode_polyline([previous_coord, depot]),
            }
        )
    return segments


def rank_delivery_insertions(
    plan: Any, delivery_id: int, limit: int
) -> list[dict[str, Any]]:
    baseline = score_components(plan)
    candidates = []
    for vehicle in plan.vehicles:
        if delivery_id in vehicle.delivery_order:
            continue
        for insert_index in range(len(vehicle.delivery_order) + 1):
            preview_plan = deepcopy(plan)
            preview_plan.normalize()
            preview_plan.remove_delivery_assignments(delivery_id)
            preview_vehicle = preview_plan.vehicles[vehicle.id]
            preview_vehicle.delivery_order.insert(insert_index, delivery_id)
            preview_plan.refresh_route_shadows()
            score = score_components(preview_plan)
            candidates.append(
                {
                    "vehicleId": vehicle.id,
                    "vehicleName": vehicle.name,
                    "insertIndex": insert_index,
                    "hardScore": score[0],
                    "softScore": score[1],
                    "score": hard_soft_string(score[0], score[1]),
                    "deltaHard": score[0] - baseline[0],
                    "deltaSoft": score[1] - baseline[1],
                    "previewPlan": preview_plan,
                }
            )
    candidates.sort(
        key=lambda item: (int(item["hardScore"]), int(item["softScore"])), reverse=True
    )
    return candidates[:limit]


def score_components(plan: Any) -> tuple[int, int]:
    components = evaluate_plan_components(plan)
    hard = -(
        components["unassignedCount"] * UNASSIGNED_DELIVERY_HARD_PENALTY
        + components["capacityOverage"] * CAPACITY_HARD_WEIGHT
        + components["lateSeconds"]
    )
    soft = -components["travelSeconds"]
    return hard, soft


def route_coord(solution: Any, vehicle: Any, element: int) -> tuple[float, float]:
    if element == len(solution.deliveries):
        return (float(vehicle.home_lat), float(vehicle.home_lng))
    delivery = solution.deliveries[element]
    return (float(delivery.lat), float(delivery.lng))


def route_bounds(plan: Any) -> dict[str, list[float]] | None:
    coords = [
        (float(delivery.lat), float(delivery.lng)) for delivery in plan.deliveries
    ] + [
        (float(vehicle.home_lat), float(vehicle.home_lng)) for vehicle in plan.vehicles
    ]
    if not coords:
        return None
    min_lat = min(lat for lat, _ in coords)
    max_lat = max(lat for lat, _ in coords)
    min_lng = min(lng for _, lng in coords)
    max_lng = max(lng for _, lng in coords)
    return {
        "southWest": [min_lat, min_lng],
        "northEast": [max_lat, max_lng],
    }


def haversine_meters(lat1: float, lng1: float, lat2: float, lng2: float) -> float:
    radius = 6_371_000
    d_lat = math.radians(lat2 - lat1)
    d_lng = math.radians(lng2 - lng1)
    a = math.sin(d_lat / 2) * math.sin(d_lat / 2) + math.cos(
        math.radians(lat1)
    ) * math.cos(math.radians(lat2)) * math.sin(d_lng / 2) * math.sin(d_lng / 2)
    return 2 * radius * math.asin(math.sqrt(a))


def meters_to_seconds(meters: int) -> int:
    meters_per_second = (AVERAGE_SPEED_KMPH * 1000) / 3600
    return round(meters / meters_per_second)


def encode_polyline(points: list[tuple[float, float]]) -> str:
    result = []
    previous_lat = 0
    previous_lng = 0
    for lat, lng in points:
        current_lat = int(round(lat * 100_000))
        current_lng = int(round(lng * 100_000))
        result.append(encode_polyline_value(current_lat - previous_lat))
        result.append(encode_polyline_value(current_lng - previous_lng))
        previous_lat = current_lat
        previous_lng = current_lng
    return "".join(result)


def encode_polyline_value(value: int) -> str:
    value = ~(value << 1) if value < 0 else value << 1
    chunks = []
    while value >= 0x20:
        chunks.append(chr((0x20 | (value & 0x1F)) + 63))
        value >>= 5
    chunks.append(chr(value + 63))
    return "".join(chunks)


def hard_soft_string(hard: int, soft: int) -> str:
    return f"{hard}hard/{soft}soft"
