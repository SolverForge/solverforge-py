from __future__ import annotations

from collections.abc import Callable
from typing import Any, TypeVar

from .fields import (
    CapacityRouteFeasibility,
    EntityCallback,
    ListMetadata,
    ListRouteHooks,
    ListSavingsHooks,
    ListValueSource,
    PlanningField,
    RowField,
    SolutionCallback,
    SolutionField,
)
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
                    "nearby_value_candidates_field": value.metadata.nearby_value_candidates_field,
                    "nearby_entity_candidates": value.metadata.nearby_entity_candidates,
                    "nearby_entity_candidates_field": value.metadata.nearby_entity_candidates_field,
                    "nearby_value_distance_meter": value.metadata.nearby_value_distance_meter,
                    "nearby_value_distance_field": value.metadata.nearby_value_distance_field,
                    "nearby_entity_distance_meter": value.metadata.nearby_entity_distance_meter,
                    "nearby_entity_distance_field": value.metadata.nearby_entity_distance_field,
                    "allows_unassigned": value.metadata.allows_unassigned,
                    "element_collection": value.metadata.element_collection,
                    "pinning": value.metadata.pinning,
                    "element_owner": value.metadata.element_owner,
                    "element_owner_field": value.metadata.element_owner_field,
                    "construction_element_order_key": value.metadata.construction_element_order_key,
                    "construction_element_order_field": value.metadata.construction_element_order_field,
                    "precedence_duration": value.metadata.precedence_duration,
                    "precedence_duration_field": value.metadata.precedence_duration_field,
                    "precedence_successors": value.metadata.precedence_successors,
                    "precedence_successors_field": value.metadata.precedence_successors_field,
                    "list_metadata": _list_metadata(value.metadata.list_metadata),
                }
            )
    return fields


def _list_metadata(metadata: ListMetadata | None) -> dict[str, object] | None:
    if metadata is None:
        return None
    return {
        "route": _route_hooks(metadata.route),
        "savings": _savings_hooks(metadata.savings),
        "cross_position_distance": _list_value_source(metadata.cross_position_distance),
        "intra_position_distance": _list_value_source(metadata.intra_position_distance),
    }


def _route_hooks(route: ListRouteHooks | None) -> dict[str, object] | None:
    if route is None:
        return None
    return {
        "depot": _list_value_source(route.depot),
        "distance": _list_value_source(route.distance),
        "feasible": _list_feasibility_source(route.feasible),
    }


def _savings_hooks(savings: ListSavingsHooks | None) -> dict[str, object] | None:
    if savings is None:
        return None
    return {
        "depot": _list_value_source(savings.depot),
        "metric_class": _list_value_source(savings.metric_class),
        "distance": _list_value_source(savings.distance),
        "feasible": _list_feasibility_source(savings.feasible),
    }


def _list_value_source(source: ListValueSource | None) -> dict[str, object] | None:
    if source is None:
        return None
    if isinstance(source, RowField):
        return {"kind": "row", "field": source.name}
    if isinstance(source, SolutionField):
        return {"kind": "solution_field", "field": source.name}
    if isinstance(source, EntityCallback):
        return {"kind": "entity", "callback": source.callback}
    if isinstance(source, SolutionCallback):
        return {"kind": "solution", "callback": source.callback}
    raise AssertionError(f"unsupported canonical list metadata source: {source!r}")


def _list_feasibility_source(source: object) -> dict[str, object]:
    if isinstance(source, CapacityRouteFeasibility):
        return {
            "kind": "capacity",
            "capacity": _list_field_source(source.capacity),
            "demand": _list_field_source(source.demand),
        }
    if isinstance(source, (EntityCallback, SolutionCallback)):
        serialized = _list_value_source(source)
        assert serialized is not None
        return serialized
    raise AssertionError(f"unsupported canonical list feasibility source: {source!r}")


def _list_field_source(source: RowField | SolutionField) -> dict[str, object]:
    serialized = _list_value_source(source)
    assert serialized is not None
    return serialized


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


def candidate_metric(
    name: str,
) -> Callable[[Callable[..., object]], Callable[..., object]]:
    if not name:
        raise ValueError("candidate metric name must not be empty")

    def decorate(fn: Callable[..., object]) -> Callable[..., object]:
        setattr(fn, "__solverforge_candidate_metric__", {"name": name})
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
    candidate_metrics: list[Callable[..., object]] | None = None,
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
                "candidate_metrics": list(candidate_metrics or []),
                "shadow_updates": update_list,
            },
        )
        return cls

    return decorate
