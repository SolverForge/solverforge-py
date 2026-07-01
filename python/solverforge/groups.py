from __future__ import annotations

from dataclasses import dataclass
from collections.abc import Callable
from typing import Any


@dataclass(frozen=True)
class ScalarGroupLimits:
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
    name: str
    entity_class: str
    variable_name: str
    required_entity: Callable[..., object] | None = None
    capacity_key: Callable[..., object] | None = None
    assignment_rule: Callable[..., object] | None = None
    position_key: Callable[..., object] | None = None
    sequence_key: Callable[..., object] | None = None
    entity_order: Callable[..., object] | None = None
    value_order: Callable[..., object] | None = None
    sync_solution_before_callbacks: bool = True
    limits: ScalarGroupLimits = ScalarGroupLimits()

    def to_native(self) -> dict[str, Any]:
        return {
            "kind": "assignment",
            "name": self.name,
            "entity_class": self.entity_class,
            "variable_name": self.variable_name,
            "required_entity": self.required_entity,
            "capacity_key": self.capacity_key,
            "assignment_rule": self.assignment_rule,
            "position_key": self.position_key,
            "sequence_key": self.sequence_key,
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
    capacity_key: Callable[..., object] | None = None,
    assignment_rule: Callable[..., object] | None = None,
    position_key: Callable[..., object] | None = None,
    sequence_key: Callable[..., object] | None = None,
    entity_order: Callable[..., object] | None = None,
    value_order: Callable[..., object] | None = None,
    sync_solution_before_callbacks: bool = True,
    limits: ScalarGroupLimits | None = None,
) -> ScalarAssignmentGroup:
    return ScalarAssignmentGroup(
        name=name,
        entity_class=entity_class,
        variable_name=variable_name,
        required_entity=required_entity,
        capacity_key=capacity_key,
        assignment_rule=assignment_rule,
        position_key=position_key,
        sequence_key=sequence_key,
        entity_order=entity_order,
        value_order=value_order,
        sync_solution_before_callbacks=sync_solution_before_callbacks,
        limits=limits or ScalarGroupLimits(),
    )
