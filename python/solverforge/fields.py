from __future__ import annotations

from dataclasses import dataclass
from collections.abc import Callable
from typing import Any


@dataclass(frozen=True)
class FieldMetadata:
    kind: str
    value_range_provider: str | None = None
    candidate_values: Callable[..., object] | None = None
    nearby_value_candidates: Callable[..., object] | None = None
    nearby_entity_candidates: Callable[..., object] | None = None
    nearby_value_distance_meter: Callable[..., object] | None = None
    nearby_entity_distance_meter: Callable[..., object] | None = None
    allows_unassigned: bool = False
    element_collection: str | None = None
    pinning: bool = False
    element_owner: Callable[..., object] | None = None
    construction_element_order_key: Callable[..., object] | None = None
    precedence_duration: Callable[..., object] | None = None
    precedence_successors: Callable[..., object] | None = None
    route_depot: Callable[..., object] | None = None
    route_depot_entity: Callable[..., object] | None = None
    route_depot_field: str | None = None
    route_metric_class: Callable[..., object] | None = None
    route_metric_class_entity: Callable[..., object] | None = None
    route_metric_class_field: str | None = None
    route_distance: Callable[..., object] | None = None
    route_distance_entity: Callable[..., object] | None = None
    route_distance_matrix_field: str | None = None
    route_feasible: Callable[..., object] | None = None
    route_feasible_entity: Callable[..., object] | None = None
    route_capacity_field: str | None = None
    route_demand_field: str | None = None


class PlanningField:
    def __init__(self, metadata: FieldMetadata) -> None:
        self.metadata = metadata
        self.name: str | None = None
        self.storage_name: str | None = None

    def __set_name__(self, owner: type[object], name: str) -> None:
        self.name = name
        self.storage_name = f"__solverforge_{name}"

    def __get__(self, instance: object | None, owner: type[object]) -> Any:
        if instance is None:
            return self
        assert self.storage_name is not None
        return getattr(instance, self.storage_name, None)

    def __set__(self, instance: object, value: Any) -> None:
        assert self.storage_name is not None
        setattr(instance, self.storage_name, value)


def planning_id() -> PlanningField:
    return PlanningField(FieldMetadata(kind="planning_id"))


def planning_variable(
    *,
    value_range_provider: str,
    candidate_values: Callable[..., object] | None = None,
    nearby_value_candidates: Callable[..., object] | None = None,
    nearby_entity_candidates: Callable[..., object] | None = None,
    nearby_value_distance_meter: Callable[..., object] | None = None,
    nearby_entity_distance_meter: Callable[..., object] | None = None,
    allows_unassigned: bool = False,
    pinning: bool = False,
) -> PlanningField:
    return PlanningField(
        FieldMetadata(
            kind="planning_variable",
            value_range_provider=value_range_provider,
            candidate_values=candidate_values,
            nearby_value_candidates=nearby_value_candidates,
            nearby_entity_candidates=nearby_entity_candidates,
            nearby_value_distance_meter=nearby_value_distance_meter,
            nearby_entity_distance_meter=nearby_entity_distance_meter,
            allows_unassigned=allows_unassigned,
            pinning=pinning,
        )
    )


def planning_list_variable(
    *,
    element_collection: str,
    element_owner: Callable[..., object] | None = None,
    construction_element_order_key: Callable[..., object] | None = None,
    precedence_duration: Callable[..., object] | None = None,
    precedence_successors: Callable[..., object] | None = None,
    route_depot: Callable[..., object] | None = None,
    route_depot_entity: Callable[..., object] | None = None,
    route_depot_field: str | None = None,
    route_metric_class: Callable[..., object] | None = None,
    route_metric_class_entity: Callable[..., object] | None = None,
    route_metric_class_field: str | None = None,
    route_distance: Callable[..., object] | None = None,
    route_distance_entity: Callable[..., object] | None = None,
    route_distance_matrix_field: str | None = None,
    route_feasible: Callable[..., object] | None = None,
    route_feasible_entity: Callable[..., object] | None = None,
    route_capacity_field: str | None = None,
    route_demand_field: str | None = None,
) -> PlanningField:
    return PlanningField(
        FieldMetadata(
            kind="planning_list_variable",
            element_collection=element_collection,
            element_owner=element_owner,
            construction_element_order_key=construction_element_order_key,
            precedence_duration=precedence_duration,
            precedence_successors=precedence_successors,
            route_depot=route_depot,
            route_depot_entity=route_depot_entity,
            route_depot_field=route_depot_field,
            route_metric_class=route_metric_class,
            route_metric_class_entity=route_metric_class_entity,
            route_metric_class_field=route_metric_class_field,
            route_distance=route_distance,
            route_distance_entity=route_distance_entity,
            route_distance_matrix_field=route_distance_matrix_field,
            route_feasible=route_feasible,
            route_feasible_entity=route_feasible_entity,
            route_capacity_field=route_capacity_field,
            route_demand_field=route_demand_field,
        )
    )
