from __future__ import annotations

from collections.abc import Callable
from typing import TypeVar

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
                    "allows_unassigned": value.metadata.allows_unassigned,
                    "element_collection": value.metadata.element_collection,
                    "pinning": value.metadata.pinning,
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
        setattr(fn, "__solverforge_conflict_repair__", {"constraints": list(constraint_names)})
        return fn

    return decorate


def planning_solution(
    *,
    score: type[object] = HardSoftScore,
    constraints: Callable[..., object] | None = None,
    scalar_groups: list[Callable[..., object]] | None = None,
    conflict_repairs: list[Callable[..., object]] | None = None,
) -> Callable[[T], T]:
    def decorate(cls: T) -> T:
        setattr(
            cls,
            "__solverforge_solution__",
            {
                "type_name": cls.__name__,
                "score_family": score_family(score),
                "constraints": constraints,
                "scalar_groups": list(scalar_groups or []),
                "conflict_repairs": list(conflict_repairs or []),
            },
        )
        return cls

    return decorate
