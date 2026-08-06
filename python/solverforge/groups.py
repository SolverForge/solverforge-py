from __future__ import annotations

from dataclasses import dataclass
from collections.abc import Callable
from typing import Any

__all__ = [
    "ScalarAssignmentGroup",
    "ScalarGroupLimits",
    "scalar_assignment_group",
]


@dataclass(frozen=True)
class ScalarGroupLimits:
    """Limits used by grouped scalar and assignment-aware scalar move selectors."""

    value_candidate_limit: int | None = None
    group_candidate_limit: int | None = None
    max_moves_per_step: int | None = None
    max_augmenting_depth: int | None = None
    max_rematch_size: int | None = None

    def to_native(self) -> dict[str, int | None]:
        return {
            "value_candidate_limit": self.value_candidate_limit,
            "group_candidate_limit": self.group_candidate_limit,
            "max_moves_per_step": self.max_moves_per_step,
            "max_augmenting_depth": self.max_augmenting_depth,
            "max_rematch_size": self.max_rematch_size,
        }


@dataclass(frozen=True)
class ScalarAssignmentGroup:
    """Assignment-aware scalar group configuration for canonical SolverForge search.

    A ``*_field`` alternative reads immutable row metadata directly from the native model.
    It is mutually exclusive with the corresponding Python callback; use it for values that do
    not depend on the evolving solution state. Construction and grouped local search are owned
    by SolverForge's runtime; an explicit phase obeys its declared termination and obligation.
    The declared variable is assignment-owned, so raw scalar selectors cannot target it alongside
    this group; compose declared grouped selectors when a phase needs multiple assignment groups.
    """

    name: str
    entity_class: str
    variable_name: str
    required_entity: Callable[..., object] | None = None
    required_entity_field: str | None = None
    capacity_key: Callable[..., object] | None = None
    capacity_key_field: str | None = None
    assignment_rule: Callable[..., object] | None = None
    same_value_conflict_field: str | None = None
    position_key: Callable[..., object] | None = None
    position_key_field: str | None = None
    sequence_key: Callable[..., object] | None = None
    sequence_key_field: str | None = None
    entity_order: Callable[..., object] | None = None
    value_order: Callable[..., object] | None = None
    sync_solution_before_callbacks: bool = True
    limits: ScalarGroupLimits = ScalarGroupLimits()

    def __post_init__(self) -> None:
        for callback, field, callback_name, field_name in (
            (
                self.required_entity,
                self.required_entity_field,
                "required_entity",
                "required_entity_field",
            ),
            (
                self.capacity_key,
                self.capacity_key_field,
                "capacity_key",
                "capacity_key_field",
            ),
            (
                self.position_key,
                self.position_key_field,
                "position_key",
                "position_key_field",
            ),
            (
                self.sequence_key,
                self.sequence_key_field,
                "sequence_key",
                "sequence_key_field",
            ),
        ):
            if callback is not None and field is not None:
                raise TypeError(
                    f"{callback_name} and {field_name} cannot both be configured"
                )
            if field is not None and (not isinstance(field, str) or not field):
                raise TypeError(
                    f"{field_name} must be a non-empty string when provided"
                )
        if (
            self.assignment_rule is not None
            and self.same_value_conflict_field is not None
        ):
            raise TypeError(
                "assignment_rule and same_value_conflict_field cannot both be configured"
            )
        if self.same_value_conflict_field is not None and (
            not isinstance(self.same_value_conflict_field, str)
            or not self.same_value_conflict_field
        ):
            raise TypeError(
                "same_value_conflict_field must be a non-empty string when provided"
            )

    def to_native(self) -> dict[str, Any]:
        return {
            "kind": "assignment",
            "name": self.name,
            "entity_class": self.entity_class,
            "variable_name": self.variable_name,
            "required_entity": self.required_entity,
            "required_entity_field": self.required_entity_field,
            "capacity_key": self.capacity_key,
            "capacity_key_field": self.capacity_key_field,
            "assignment_rule": self.assignment_rule,
            "same_value_conflict_field": self.same_value_conflict_field,
            "position_key": self.position_key,
            "position_key_field": self.position_key_field,
            "sequence_key": self.sequence_key,
            "sequence_key_field": self.sequence_key_field,
            "entity_order": self.entity_order,
            "value_order": self.value_order,
            "sync_solution_before_callbacks": self.sync_solution_before_callbacks,
            "limits": self.limits.to_native(),
        }


def scalar_assignment_group(
    name: str,
    *,
    entity_class: str,
    variable_name: str,
    required_entity: Callable[..., object] | None = None,
    required_entity_field: str | None = None,
    capacity_key: Callable[..., object] | None = None,
    capacity_key_field: str | None = None,
    assignment_rule: Callable[..., object] | None = None,
    same_value_conflict_field: str | None = None,
    position_key: Callable[..., object] | None = None,
    position_key_field: str | None = None,
    sequence_key: Callable[..., object] | None = None,
    sequence_key_field: str | None = None,
    entity_order: Callable[..., object] | None = None,
    value_order: Callable[..., object] | None = None,
    sync_solution_before_callbacks: bool = True,
    limits: ScalarGroupLimits | None = None,
) -> ScalarAssignmentGroup:
    """Declare an assignment-aware scalar group.

    ``required_entity_field`` is a row bool, ``position_key_field`` and
    ``sequence_key_field`` are non-negative row integers, and ``capacity_key_field`` is a
    row list of optional integers indexed by the candidate value. These sources avoid Python
    callback transitions while preserving the declared metadata values. The declaration compiles
    into one immutable native runtime plan rather than a Python-side assignment search path. Its
    target variable is exclusively searched through the resulting grouped selector.
    ``same_value_conflict_field`` names a row list of adjacent-sequence entity indices that may
    not share the same assigned value, providing a native conflict-graph alternative to
    ``assignment_rule``.
    """

    return ScalarAssignmentGroup(
        name=name,
        entity_class=entity_class,
        variable_name=variable_name,
        required_entity=required_entity,
        required_entity_field=required_entity_field,
        capacity_key=capacity_key,
        capacity_key_field=capacity_key_field,
        assignment_rule=assignment_rule,
        same_value_conflict_field=same_value_conflict_field,
        position_key=position_key,
        position_key_field=position_key_field,
        sequence_key=sequence_key,
        sequence_key_field=sequence_key_field,
        entity_order=entity_order,
        value_order=value_order,
        sync_solution_before_callbacks=sync_solution_before_callbacks,
        limits=limits or ScalarGroupLimits(),
    )
