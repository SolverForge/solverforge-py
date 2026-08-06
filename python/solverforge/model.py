from __future__ import annotations

from collections.abc import Callable, Iterable
from functools import partial
from sys import modules
from types import FunctionType, MethodType
from typing import TYPE_CHECKING, Any, cast, get_args, get_origin, get_type_hints
from weakref import WeakKeyDictionary

from .constraints import ConstraintFactory, ConstraintPlan
from .errors import ModelValidationError
from .groups import ScalarAssignmentGroup

if TYPE_CHECKING:
    from . import _native

SchemaShape = tuple[object, ...]

_COMPILED_SCHEMA_CACHE: WeakKeyDictionary[
    type[object], dict[SchemaShape, _native.CompiledSchema]
] = WeakKeyDictionary()


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


def _schema_shape(value: object) -> tuple[SchemaShape, bool]:
    active: dict[int, int] = {}
    next_marker = 0
    cacheable = True

    def shape(item: object, *, callback_state: bool = False) -> SchemaShape:
        nonlocal cacheable, next_marker
        recursive = isinstance(
            item, (dict, list, tuple, FunctionType, MethodType, partial)
        )
        item_id = id(item)
        if recursive:
            if item_id in active:
                return ("cycle", active[item_id])
            active[item_id] = next_marker
            next_marker += 1
        try:
            if isinstance(item, dict):
                if callback_state:
                    cacheable = False
                if any(not isinstance(key, str) for key in item):
                    # Raw schemas are string-keyed.  Do not call __str__ on an
                    # arbitrary key just to manufacture a cache key: it can
                    # both conflate distinct keys and execute user code.  This
                    # schema is deliberately compiled for this invocation.
                    cacheable = False
                    return ("dict_non_string_key", id(item))
                return (
                    "dict",
                    *(
                        (key, shape(child, callback_state=callback_state))
                        for key, child in sorted(item.items())
                    ),
                )
            if isinstance(item, (list, tuple)):
                if callback_state:
                    cacheable = False
                return (
                    type(item).__name__,
                    *(shape(child, callback_state=callback_state) for child in item),
                )
            if isinstance(item, type):
                return ("type", item)
            if isinstance(item, FunctionType):
                module = modules.get(item.__module__)
                has_callback_state = bool(
                    item.__closure__
                    or item.__defaults__
                    or item.__kwdefaults__
                    or item.__dict__
                    or item.__annotations__
                )
                if (
                    callback_state
                    or has_callback_state
                    or module is None
                    or vars(module) is not item.__globals__
                ):
                    cacheable = False
                closure = tuple(
                    shape(cell.cell_contents, callback_state=True)
                    for cell in (item.__closure__ or ())
                )
                defaults = tuple(
                    shape(default, callback_state=True)
                    for default in (item.__defaults__ or ())
                )
                keyword_defaults = tuple(
                    (name, shape(default, callback_state=True))
                    for name, default in sorted((item.__kwdefaults__ or {}).items())
                )
                return (
                    "function",
                    item.__code__,
                    module,
                    id(getattr(item, "__builtins__", None)),
                    defaults,
                    keyword_defaults,
                    closure,
                )
            if isinstance(item, partial):
                cacheable = False
                return (
                    "partial",
                    shape(item.func),
                    shape(item.args, callback_state=True),
                    shape(item.keywords, callback_state=True),
                )
            if isinstance(item, MethodType):
                cacheable = False
                return ("method", shape(item.__func__), id(item.__self__))
            if callable(item):
                cacheable = False
                return ("callable", type(item), id(item))
            if item is None or isinstance(item, (bool, int, float, str)):
                if callback_state:
                    cacheable = False
                return ("value", type(item).__name__, item)
            cacheable = False
            return ("object", type(item), id(item))
        finally:
            if recursive:
                del active[item_id]

    return shape(value), cacheable


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
    names: set[str] = set()
    for item in cast(Iterable[object], callbacks or []):
        if isinstance(item, ScalarAssignmentGroup):
            if not item.name:
                msg = f"{item!r} has an invalid scalar group name"
                raise ModelValidationError(msg)
            if (
                item.assignment_rule is not None
                and item.sequence_key is None
                and item.sequence_key_field is None
            ):
                msg = (
                    f"{item!r} declares an assignment_rule but no sequence_key; "
                    "assignment-rule groups need sequence metadata"
                )
                raise ModelValidationError(msg)
            if (
                item.same_value_conflict_field is not None
                and item.sequence_key is None
                and item.sequence_key_field is None
            ):
                msg = (
                    f"{item!r} declares a same_value_conflict_field but no sequence_key; "
                    "same-value conflict groups need sequence metadata"
                )
                raise ModelValidationError(msg)
            if item.name in names:
                msg = f"scalar group name `{item.name}` is declared more than once"
                raise ModelValidationError(msg)
            names.add(item.name)
            groups.append(item.to_native())
            continue
        callback = cast(Callable[..., object], item)
        metadata = getattr(callback, "__solverforge_scalar_group__", None)
        if not isinstance(metadata, dict):
            msg = f"{callback!r} is not marked with @scalar_group and is not a ScalarAssignmentGroup"
            raise ModelValidationError(msg)
        name = metadata.get("name")
        if not isinstance(name, str) or not name:
            msg = f"{callback!r} has an invalid scalar group name"
            raise ModelValidationError(msg)
        if name in names:
            msg = f"scalar group name `{name}` is declared more than once"
            raise ModelValidationError(msg)
        names.add(name)
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


def _candidate_metrics(callbacks: object) -> list[dict[str, object]]:
    metrics: list[dict[str, object]] = []
    names: set[str] = set()
    for callback in cast(Iterable[Callable[..., object]], callbacks or []):
        metadata = getattr(callback, "__solverforge_candidate_metric__", None)
        if not isinstance(metadata, dict):
            msg = f"{callback!r} is not marked with @candidate_metric"
            raise ModelValidationError(msg)
        name = metadata.get("name")
        if not isinstance(name, str) or not name:
            msg = f"{callback!r} has an invalid candidate metric name"
            raise ModelValidationError(msg)
        if name in names:
            msg = f"candidate metric name `{name}` is declared more than once"
            raise ModelValidationError(msg)
        names.add(name)
        metrics.append({"name": name, "callback": callback})
    return metrics


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
        "candidate_metrics": _candidate_metrics(solution_meta.get("candidate_metrics")),
        "shadow_updates": list(
            cast(Iterable[dict[str, object]], solution_meta.get("shadow_updates") or [])
        ),
    }


def _compiled_schema_for_solution(solution: object) -> _native.CompiledSchema:
    """Reuse compiled schemas only when callback state is structurally stable."""

    solution_type = type(solution)
    from . import _native

    schema = build_schema(solution)
    shape, cacheable = _schema_shape(schema)
    if not cacheable:
        return _native.compile_schema(schema)
    cached_by_shape = _COMPILED_SCHEMA_CACHE.setdefault(solution_type, {})
    cached = cached_by_shape.get(shape)
    if cached is not None:
        return cached
    compiled = _native.compile_schema(schema)
    cached_by_shape[shape] = compiled
    return compiled
