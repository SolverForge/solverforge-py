from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any, Self, cast

from .score import score_to_native


@dataclass(frozen=True)
class ConstraintPlan:
    entity_type: type[object]
    score_family: str = "hard_soft"
    constraint_type: str | None = None
    variable_name: str | None = None
    arity: int = 1
    left_filters: tuple[Callable[..., bool], ...] = ()
    right_filters: tuple[Callable[..., bool], ...] = ()
    filters: tuple[Callable[..., bool], ...] = ()
    group_filters: tuple[Callable[..., bool], ...] = ()
    impact: str = "penalty"
    weight: object = 1
    name: str | None = None
    right_entity_type: type[object] | None = None
    joiners: tuple[object, ...] = ()
    group_key: Callable[..., object] | None = None
    group_collector: object | None = None
    balance_key: Callable[..., object] | None = None
    precedence_duration: Callable[..., object] | None = None
    precedence_duration_field: str | None = None
    precedence_successors: Callable[..., object] | None = None
    precedence_successors_field: str | None = None
    element_owner: Callable[..., object] | None = None
    element_owner_field: str | None = None

    def to_native(self) -> dict[str, object]:
        plan: dict[str, object] = {
            "arity": self.arity,
            "entity_type": self.entity_type.__name__,
            "score_family": self.score_family,
            "left_filters": list(self.left_filters),
            "right_filters": list(self.right_filters),
            "filters": list(self.filters),
            "group_filters": list(self.group_filters),
            "impact": self.impact,
            "name": self.name or f"{self.entity_type.__name__} constraint",
        }
        if self.right_entity_type is not None:
            plan["right_entity_type"] = self.right_entity_type.__name__
        if self.constraint_type is not None:
            plan["constraint_type"] = self.constraint_type
        if self.variable_name is not None:
            plan["variable_name"] = self.variable_name
            plan["element_collection"] = _list_variable_element_collection(
                self.entity_type,
                self.variable_name,
            )
        if self.joiners:
            plan["joiners"] = [joiner_to_native(joiner) for joiner in self.joiners]
        if self.group_key is not None:
            plan["group_key"] = self.group_key
        if self.group_collector is not None:
            plan["group_collector"] = collector_to_native(self.group_collector)
        if self.balance_key is not None:
            plan["balance_key"] = self.balance_key
        if self.precedence_duration is not None:
            plan["precedence_duration"] = self.precedence_duration
        if self.precedence_duration_field is not None:
            plan["precedence_duration_field"] = self.precedence_duration_field
        if self.precedence_successors is not None:
            plan["precedence_successors"] = self.precedence_successors
        if self.precedence_successors_field is not None:
            plan["precedence_successors_field"] = self.precedence_successors_field
        if self.element_owner is not None:
            plan["element_owner"] = self.element_owner
        if self.element_owner_field is not None:
            plan["element_owner_field"] = self.element_owner_field
        if callable(self.weight):
            plan["weight"] = _callback_weight_placeholder(self.score_family)
            plan["weight_callback"] = self.weight
        else:
            plan["weight"] = score_to_native(self.weight, self.score_family)
        return plan


@dataclass
class UnassignedListElementConstraintStream:
    entity_type: type[object]
    variable_name: str
    score_family: str = "hard_soft"
    filters: list[Callable[..., bool]] = field(default_factory=list)
    impact: str | None = None
    weight: object | None = None

    def filter(self, predicate: Callable[..., bool]) -> Self:
        self.filters.append(predicate)
        return self

    def penalize(self, weight: object) -> Self:
        self.impact = "penalty"
        self.weight = weight
        return self

    def reward(self, weight: object) -> Self:
        self.impact = "reward"
        self.weight = weight
        return self

    def named(self, name: str) -> ConstraintPlan:
        return ConstraintPlan(
            entity_type=self.entity_type,
            score_family=self.score_family,
            constraint_type="list_unassigned_element",
            variable_name=self.variable_name,
            filters=tuple(self.filters),
            impact=self.impact or "penalty",
            weight=self.weight if self.weight is not None else 1,
            name=name,
        )


@dataclass
class ListPrecedenceMakespanConstraintStream:
    entity_type: type[object]
    variable_name: str
    score_family: str = "hard_soft"

    def named(self, name: str) -> ConstraintPlan:
        metadata = _list_variable_metadata(self.entity_type, self.variable_name)
        plan = ConstraintPlan(
            entity_type=self.entity_type,
            score_family=self.score_family,
            constraint_type="list_precedence_makespan",
            variable_name=self.variable_name,
            impact="penalty",
            weight=_zero_weight(self.score_family),
            name=name,
            precedence_duration=_metadata_callback(
                metadata,
                "precedence_duration",
            ),
            precedence_duration_field=_metadata_field_name(
                metadata,
                "precedence_duration_field",
            ),
            precedence_successors=_metadata_callback(
                metadata,
                "precedence_successors",
            ),
            precedence_successors_field=_metadata_field_name(
                metadata,
                "precedence_successors_field",
            ),
            element_owner=_metadata_callback(
                metadata,
                "element_owner",
            ),
            element_owner_field=_metadata_field_name(
                metadata,
                "element_owner_field",
            ),
        )
        if plan.precedence_duration is None and plan.precedence_duration_field is None:
            msg = f"{self.entity_type.__name__}.{self.variable_name} requires precedence_duration"
            raise TypeError(msg)
        if (
            plan.precedence_successors is None
            and plan.precedence_successors_field is None
        ):
            msg = f"{self.entity_type.__name__}.{self.variable_name} requires precedence_successors"
            raise TypeError(msg)
        if plan.element_owner is None and plan.element_owner_field is None:
            msg = f"{self.entity_type.__name__}.{self.variable_name} requires element_owner"
            raise TypeError(msg)
        return plan


@dataclass
class UniConstraintStream:
    entity_type: type[object]
    score_family: str = "hard_soft"
    filters: list[Callable[..., bool]] = field(default_factory=list)
    impact: str | None = None
    weight: object | None = None

    def filter(self, predicate: Callable[..., bool]) -> Self:
        self.filters.append(predicate)
        return self

    def penalize(self, weight: object) -> Self:
        self.impact = "penalty"
        self.weight = weight
        return self

    def reward(self, weight: object) -> Self:
        self.impact = "reward"
        self.weight = weight
        return self

    def join(
        self,
        target: object,
        *joiners: object,
    ) -> BiConstraintStream:
        right_filters: list[Callable[..., bool]] = []
        if isinstance(target, tuple):
            if not target:
                msg = "join target tuple must not be empty"
                raise TypeError(msg)
            stream_or_type, *tuple_joiners = target
            target = stream_or_type
            joiners = (*tuple_joiners, *joiners)
        if isinstance(target, UniConstraintStream):
            right_entity_type = target.entity_type
            right_filters = list(target.filters)
        elif isinstance(target, type):
            right_entity_type = target
        elif hasattr(target, "to_native"):
            right_entity_type = self.entity_type
            joiners = (target, *joiners)
        else:
            msg = f"unsupported join target {target!r}"
            raise TypeError(msg)
        return BiConstraintStream(
            entity_type=self.entity_type,
            score_family=self.score_family,
            right_entity_type=right_entity_type,
            left_filters=list(self.filters),
            right_filters=right_filters,
            joiners=list(joiners),
        )

    def group_by(
        self, key: Callable[[object], object], collector: object | None = None
    ) -> GroupedConstraintStream:
        return GroupedConstraintStream(
            entity_type=self.entity_type,
            score_family=self.score_family,
            pre_filters=list(self.filters),
            group_key=key,
            group_collector=collector,
        )

    def balance(self, key: Callable[[object], object]) -> BalanceConstraintStream:
        return BalanceConstraintStream(
            entity_type=self.entity_type,
            score_family=self.score_family,
            pre_filters=list(self.filters),
            balance_key=key,
        )

    def named(self, name: str) -> ConstraintPlan:
        return ConstraintPlan(
            entity_type=self.entity_type,
            score_family=self.score_family,
            filters=tuple(self.filters),
            impact=self.impact or "penalty",
            weight=self.weight if self.weight is not None else 1,
            name=name,
        )


@dataclass
class BiConstraintStream:
    entity_type: type[object]
    score_family: str
    right_entity_type: type[object]
    left_filters: list[Callable[..., bool]] = field(default_factory=list)
    right_filters: list[Callable[..., bool]] = field(default_factory=list)
    filters: list[Callable[..., bool]] = field(default_factory=list)
    joiners: list[object] = field(default_factory=list)
    impact: str | None = None
    weight: object | None = None

    def filter(self, predicate: Callable[..., bool]) -> Self:
        self.filters.append(predicate)
        return self

    def penalize(self, weight: object) -> Self:
        self.impact = "penalty"
        self.weight = weight
        return self

    def reward(self, weight: object) -> Self:
        self.impact = "reward"
        self.weight = weight
        return self

    def named(self, name: str) -> ConstraintPlan:
        return ConstraintPlan(
            entity_type=self.entity_type,
            score_family=self.score_family,
            arity=2,
            left_filters=tuple(self.left_filters),
            right_filters=tuple(self.right_filters),
            filters=tuple(self.filters),
            impact=self.impact or "penalty",
            weight=self.weight if self.weight is not None else 1,
            name=name,
            right_entity_type=self.right_entity_type,
            joiners=tuple(self.joiners),
        )


@dataclass
class GroupedConstraintStream:
    entity_type: type[object]
    score_family: str
    group_key: Callable[[object], object]
    group_collector: object | None = None
    pre_filters: list[Callable[..., bool]] = field(default_factory=list)
    filters: list[Callable[..., bool]] = field(default_factory=list)
    impact: str | None = None
    weight: object | None = None

    def filter(self, predicate: Callable[..., bool]) -> Self:
        self.filters.append(predicate)
        return self

    def penalize(self, weight: object) -> Self:
        self.impact = "penalty"
        self.weight = weight
        return self

    def reward(self, weight: object) -> Self:
        self.impact = "reward"
        self.weight = weight
        return self

    def named(self, name: str) -> ConstraintPlan:
        return ConstraintPlan(
            entity_type=self.entity_type,
            score_family=self.score_family,
            filters=tuple(self.pre_filters),
            group_filters=tuple(self.filters),
            impact=self.impact or "penalty",
            weight=self.weight if self.weight is not None else 1,
            name=name,
            group_key=self.group_key,
            group_collector=self.group_collector,
        )


@dataclass
class BalanceConstraintStream:
    entity_type: type[object]
    score_family: str
    balance_key: Callable[[object], object]
    pre_filters: list[Callable[..., bool]] = field(default_factory=list)
    filters: list[Callable[..., bool]] = field(default_factory=list)
    impact: str | None = None
    weight: object | None = None

    def filter(self, predicate: Callable[..., bool]) -> Self:
        self.filters.append(predicate)
        return self

    def penalize(self, weight: object) -> Self:
        self.impact = "penalty"
        self.weight = weight
        return self

    def reward(self, weight: object) -> Self:
        self.impact = "reward"
        self.weight = weight
        return self

    def named(self, name: str) -> ConstraintPlan:
        return ConstraintPlan(
            entity_type=self.entity_type,
            score_family=self.score_family,
            filters=tuple(self.pre_filters + self.filters),
            impact=self.impact or "penalty",
            weight=self.weight if self.weight is not None else 1,
            name=name,
            balance_key=self.balance_key,
        )


class ConstraintFactory:
    def __init__(self, *, score_family: str = "hard_soft") -> None:
        self.score_family = score_family

    def for_each(self, entity_type: type[object]) -> UniConstraintStream:
        return UniConstraintStream(
            entity_type=entity_type, score_family=self.score_family
        )

    def for_each_unassigned_element(
        self,
        owner_entity_type: type[object],
        variable_name: str,
    ) -> UnassignedListElementConstraintStream:
        return UnassignedListElementConstraintStream(
            entity_type=owner_entity_type,
            variable_name=variable_name,
            score_family=self.score_family,
        )

    def list_precedence_makespan(
        self,
        owner_entity_type: type[object],
        variable_name: str,
    ) -> ListPrecedenceMakespanConstraintStream:
        return ListPrecedenceMakespanConstraintStream(
            entity_type=owner_entity_type,
            variable_name=variable_name,
            score_family=self.score_family,
        )

    def join(self, *_args: Any, **_kwargs: Any) -> None:
        raise NotImplementedError(
            "dynamic join is implemented in the native stream planner"
        )

    def if_exists(self, *_args: Any, **_kwargs: Any) -> None:
        raise NotImplementedError(
            "dynamic if_exists is implemented in the native stream planner"
        )

    def if_not_exists(self, *_args: Any, **_kwargs: Any) -> None:
        raise NotImplementedError(
            "dynamic if_not_exists is implemented in the native stream planner"
        )

    def group_by(self, *_args: Any, **_kwargs: Any) -> None:
        raise NotImplementedError(
            "dynamic group_by is implemented in the native stream planner"
        )

    def flattened(self, *_args: Any, **_kwargs: Any) -> None:
        raise NotImplementedError(
            "dynamic flattened is implemented in the native stream planner"
        )


def joiner_to_native(joiner: object) -> object:
    if hasattr(joiner, "to_native"):
        native = joiner.to_native()
        if isinstance(native, dict):
            return native
    return joiner


@dataclass(frozen=True)
class IndexedPresenceCollector:
    index: Callable[[object], int]

    def to_native(self) -> dict[str, object]:
        return {"type": "indexed_presence", "index": self.index}


def indexed_presence(index: Callable[[object], int]) -> IndexedPresenceCollector:
    return IndexedPresenceCollector(index=index)


def collector_to_native(collector: object) -> object:
    if hasattr(collector, "to_native"):
        native = collector.to_native()
        if isinstance(native, dict):
            return native
    return collector


def _list_variable_element_collection(
    entity_type: type[object], variable_name: str
) -> str:
    field_info = _list_variable_metadata(entity_type, variable_name)
    collection = field_info.get("element_collection")
    if isinstance(collection, str) and collection:
        return collection
    msg = f"{entity_type.__name__}.{variable_name} has no element collection"
    raise TypeError(msg)


def _list_variable_metadata(
    entity_type: type[object], variable_name: str
) -> dict[str, object]:
    metadata = getattr(entity_type, "__solverforge_entity__", None)
    if not isinstance(metadata, dict):
        msg = f"{entity_type.__name__} is not marked with @planning_entity"
        raise TypeError(msg)
    for field_info in metadata.get("fields", []):
        if not isinstance(field_info, dict):
            continue
        if (
            field_info.get("name") == variable_name
            and field_info.get("kind") == "planning_list_variable"
        ):
            return field_info
    msg = f"{entity_type.__name__}.{variable_name} is not a planning list variable"
    raise TypeError(msg)


def _metadata_callback(
    metadata: dict[str, object],
    field_name: str,
) -> Callable[..., object] | None:
    value = metadata.get(field_name)
    if callable(value):
        return cast(Callable[..., object], value)
    return None


def _metadata_field_name(metadata: dict[str, object], field_name: str) -> str | None:
    value = metadata.get(field_name)
    if isinstance(value, str) and value:
        return value
    return None


def _callback_weight_placeholder(score_family: str) -> dict[str, object]:
    match score_family:
        case "soft":
            return {"family": "soft", "levels": [1]}
        case "hard_soft":
            return {"family": "hard_soft", "levels": [1, 0]}
        case "hard_soft_decimal":
            return {"family": "hard_soft_decimal", "levels": [1, 0]}
        case "hard_medium_soft":
            return {"family": "hard_medium_soft", "levels": [1, 0, 0]}
        case _:
            return score_to_native(1)


def _zero_weight(score_family: str) -> object:
    match score_family:
        case "soft":
            return [0]
        case "hard_soft":
            return [0, 0]
        case "hard_soft_decimal":
            return [0, 0]
        case "hard_medium_soft":
            return [0, 0, 0]
        case _:
            return [0]
