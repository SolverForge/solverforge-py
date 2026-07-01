from __future__ import annotations

from collections.abc import Callable, Iterable
from typing import Any, cast, get_args, get_origin, get_type_hints

from .constraints import ConstraintFactory, ConstraintPlan
from .errors import ModelValidationError
from .groups import ScalarAssignmentGroup


def _entity_metadata(entity_type: type[object]) -> dict[str, object]:
    metadata = getattr(entity_type, "__solverforge_entity__", None)
    if not isinstance(metadata, dict):
        msg = f"{entity_type.__name__} is not marked with @planning_entity"
        raise ModelValidationError(msg)
    return metadata


def _fact_metadata(fact_type: type[object]) -> dict[str, object]:
    metadata = getattr(fact_type, "__solverforge_fact__", None)
    if not isinstance(metadata, dict):
        msg = f"{fact_type.__name__} is not marked with @problem_fact"
        raise ModelValidationError(msg)
    return metadata


def _infer_entity_collections(solution: object) -> list[dict[str, object]]:
    entities: list[dict[str, object]] = []
    annotations = get_type_hints(type(solution), include_extras=True)
    for field_name, annotation in annotations.items():
        origin = get_origin(annotation)
        args = get_args(annotation)
        if origin is list and args:
            item_type = args[0]
            if hasattr(item_type, "__solverforge_entity__"):
                metadata = dict(_entity_metadata(item_type))
                metadata["collection"] = field_name
                entities.append(metadata)
    if not entities:
        for field_name, value in vars(solution).items():
            if isinstance(value, list) and value:
                item_type = type(value[0])
                if hasattr(item_type, "__solverforge_entity__"):
                    metadata = dict(_entity_metadata(item_type))
                    metadata["collection"] = field_name
                    entities.append(metadata)
    return entities


def _infer_fact_collections(solution: object) -> list[dict[str, object]]:
    facts: list[dict[str, object]] = []
    annotations = get_type_hints(type(solution), include_extras=True)
    for field_name, annotation in annotations.items():
        origin = get_origin(annotation)
        args = get_args(annotation)
        if origin is list and args:
            item_type = args[0]
            if hasattr(item_type, "__solverforge_fact__"):
                metadata = dict(_fact_metadata(item_type))
                metadata["collection"] = field_name
                facts.append(metadata)
    if not facts:
        for field_name, value in vars(solution).items():
            if isinstance(value, list) and value:
                item_type = type(value[0])
                if hasattr(item_type, "__solverforge_fact__"):
                    metadata = dict(_fact_metadata(item_type))
                    metadata["collection"] = field_name
                    facts.append(metadata)
    return facts


def _constraint_plans(
    provider: Callable[..., object] | None,
    score_family: str,
) -> list[dict[str, object]]:
    if provider is None:
        return []
    produced = provider(ConstraintFactory(score_family=score_family))
    if isinstance(produced, ConstraintPlan):
        plans = [produced]
    else:
        plans = list(cast(Iterable[ConstraintPlan], produced))
    return [plan.to_native() for plan in plans]


def _scalar_groups(callbacks: object) -> list[dict[str, object]]:
    groups: list[dict[str, object]] = []
    for item in cast(Iterable[object], callbacks or []):
        if isinstance(item, ScalarAssignmentGroup):
            if not item.name:
                msg = f"{item!r} has an invalid scalar group name"
                raise ModelValidationError(msg)
            if item.assignment_rule is not None and item.sequence_key is None:
                msg = (
                    f"{item!r} declares an assignment_rule but no sequence_key; "
                    "assignment-rule groups need sequence metadata"
                )
                raise ModelValidationError(msg)
            groups.append(item.to_native())
            continue
        callback = cast(Callable[..., object], item)
        metadata = getattr(callback, "__solverforge_scalar_group__", None)
        if not isinstance(metadata, dict):
            msg = (
                f"{callback!r} is not marked with @scalar_group and is not a "
                "ScalarAssignmentGroup"
            )
            raise ModelValidationError(msg)
        name = metadata.get("name")
        if not isinstance(name, str) or not name:
            msg = f"{callback!r} has an invalid scalar group name"
            raise ModelValidationError(msg)
        groups.append({"kind": "callback", "name": name, "callback": callback})
    return groups


def _conflict_repairs(callbacks: object) -> list[dict[str, object]]:
    repairs: list[dict[str, object]] = []
    for callback in cast(Iterable[Callable[..., object]], callbacks or []):
        metadata = getattr(callback, "__solverforge_conflict_repair__", None)
        if not isinstance(metadata, dict):
            msg = f"{callback!r} is not marked with @conflict_repair"
            raise ModelValidationError(msg)
        constraint_names = list(
            cast(Iterable[object], metadata.get("constraints") or [])
        )
        if not constraint_names or not all(
            isinstance(name, str) and name for name in constraint_names
        ):
            msg = f"{callback!r} must declare at least one conflict repair constraint name"
            raise ModelValidationError(msg)
        repairs.append({"constraints": constraint_names, "callback": callback})
    return repairs


def build_schema(solution: object) -> dict[str, Any]:
    solution_meta = getattr(type(solution), "__solverforge_solution__", None)
    if not isinstance(solution_meta, dict):
        msg = f"{type(solution).__name__} is not marked with @planning_solution"
        raise ModelValidationError(msg)
    score_family = cast(str, solution_meta["score_family"])
    constraints = _constraint_plans(solution_meta.get("constraints"), score_family)
    return {
        "solution_type": solution_meta["type_name"],
        "score_family": score_family,
        "score_field": "score",
        "entities": _infer_entity_collections(solution),
        "facts": _infer_fact_collections(solution),
        "constraints": constraints,
        "scalar_groups": _scalar_groups(solution_meta.get("scalar_groups")),
        "conflict_repairs": _conflict_repairs(solution_meta.get("conflict_repairs")),
        "shadow_updates": list(
            cast(Iterable[dict[str, object]], solution_meta.get("shadow_updates") or [])
        ),
    }
