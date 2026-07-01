from __future__ import annotations

from collections.abc import Callable
from typing import Any, TypeVar

from .fields import PlanningField
from .score import HardSoftScore, score_family

T = TypeVar("T", bound=type[object])


def _collect_fields(cls: type[object]) -> list[dict[str, object]]:
    fields: list[dict[str, object]] = []
    for name, value in vars(cls).items():
        if isinstance(value, PlanningField):
            fields.append(
                {
                    "name": name,
                    "kind": value.metadata.kind,
                    "value_range_provider": value.metadata.value_range_provider,
                    "candidate_values": value.metadata.candidate_values,
                    "nearby_value_candidates": value.metadata.nearby_value_candidates,
                    "nearby_entity_candidates": value.metadata.nearby_entity_candidates,
                    "nearby_value_distance_meter": value.metadata.nearby_value_distance_meter,
                    "nearby_entity_distance_meter": value.metadata.nearby_entity_distance_meter,
                    "allows_unassigned": value.metadata.allows_unassigned,
                    "element_collection": value.metadata.element_collection,
                    "pinning": value.metadata.pinning,
                    "element_owner": value.metadata.element_owner,
                    "construction_element_order_key": value.metadata.construction_element_order_key,
                    "precedence_duration": value.metadata.precedence_duration,
                    "precedence_successors": value.metadata.precedence_successors,
                    "route_depot": value.metadata.route_depot,
                    "route_depot_entity": value.metadata.route_depot_entity,
                    "route_depot_field": value.metadata.route_depot_field,
                    "route_metric_class": value.metadata.route_metric_class,
                    "route_metric_class_entity": value.metadata.route_metric_class_entity,
                    "route_metric_class_field": value.metadata.route_metric_class_field,
                    "route_distance": value.metadata.route_distance,
                    "route_distance_entity": value.metadata.route_distance_entity,
                    "route_distance_matrix_field": value.metadata.route_distance_matrix_field,
                    "route_feasible": value.metadata.route_feasible,
                    "route_feasible_entity": value.metadata.route_feasible_entity,
                    "route_capacity_field": value.metadata.route_capacity_field,
                    "route_demand_field": value.metadata.route_demand_field,
                }
            )
    return fields


def planning_entity(cls: T) -> T:
    setattr(
        cls,
        "__solverforge_entity__",
        {
            "type_name": cls.__name__,
            "fields": _collect_fields(cls),
        },
    )
    return cls


def problem_fact(cls: T) -> T:
    setattr(
        cls,
        "__solverforge_fact__",
        {
            "type_name": cls.__name__,
            "fields": _collect_fields(cls),
        },
    )
    return cls


def constraint_provider(fn: Callable[..., object]) -> Callable[..., object]:
    setattr(fn, "__solverforge_constraint_provider__", True)
    return fn


def scalar_group(name: str) -> Callable[[Callable[..., object]], Callable[..., object]]:
    def decorate(fn: Callable[..., object]) -> Callable[..., object]:
        setattr(fn, "__solverforge_scalar_group__", {"name": name})
        return fn

    return decorate


def conflict_repair(
    *constraint_names: str,
) -> Callable[[Callable[..., object]], Callable[..., object]]:
    def decorate(fn: Callable[..., object]) -> Callable[..., object]:
        setattr(
            fn,
            "__solverforge_conflict_repair__",
            {"constraints": list(constraint_names)},
        )
        return fn

    return decorate


def shadow_variable_updates(
    *,
    list_owner: str,
    post_update_listener: Callable[..., object],
) -> dict[str, object]:
    return {
        "list_owner": list_owner,
        "post_update_listener": post_update_listener,
    }


def planning_solution(
    *,
    score: type[object] = HardSoftScore,
    constraints: Callable[..., object] | None = None,
    scalar_groups: list[Callable[..., object]] | None = None,
    conflict_repairs: list[Callable[..., object]] | None = None,
    shadow_updates: dict[str, Any] | list[dict[str, Any]] | None = None,
    shadow_variable_updates: dict[str, Any] | list[dict[str, Any]] | None = None,
) -> Callable[[T], T]:
    def decorate(cls: T) -> T:
        updates = (
            shadow_updates if shadow_updates is not None else shadow_variable_updates
        )
        if updates is None:
            update_list: list[dict[str, Any]] = []
        elif isinstance(updates, dict):
            update_list = [updates]
        else:
            update_list = list(updates)
        setattr(
            cls,
            "__solverforge_solution__",
            {
                "type_name": cls.__name__,
                "score_family": score_family(score),
                "constraints": constraints,
                "scalar_groups": list(scalar_groups or []),
                "conflict_repairs": list(conflict_repairs or []),
                "shadow_updates": update_list,
            },
        )
        return cls

    return decorate
