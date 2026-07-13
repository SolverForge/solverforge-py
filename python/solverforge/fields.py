from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from inspect import Parameter, signature
from typing import Any, NamedTuple, TypeAlias

MetadataSource = Callable[..., object] | str


class ResolvedMetadataSource(NamedTuple):
    callback: Callable[..., object] | None
    field_name: str | None


@dataclass(frozen=True)
class RowField:
    """An immutable metadata field read from the owning entity row."""

    name: str

    def __post_init__(self) -> None:
        if not isinstance(self.name, str) or not self.name:
            raise TypeError("RowField name must be a non-empty string")


@dataclass(frozen=True)
class SolutionField:
    """An immutable metadata field read from the declared solution root."""

    name: str

    def __post_init__(self) -> None:
        if not isinstance(self.name, str) or not self.name:
            raise TypeError("SolutionField name must be a non-empty string")


@dataclass(frozen=True)
class EntityCallback:
    """A callback whose first argument is the owning entity row."""

    callback: Callable[..., object]

    def __post_init__(self) -> None:
        if not callable(self.callback):
            raise TypeError("EntityCallback callback must be callable")


@dataclass(frozen=True)
class SolutionCallback:
    """A callback whose first arguments are the solution and entity index."""

    callback: Callable[..., object]

    def __post_init__(self) -> None:
        if not callable(self.callback):
            raise TypeError("SolutionCallback callback must be callable")


ListValueSource: TypeAlias = (
    RowField | SolutionField | EntityCallback | SolutionCallback
)
ListFeasibilityFieldSource: TypeAlias = RowField | SolutionField


@dataclass(frozen=True)
class CapacityRouteFeasibility:
    """Native capacity feasibility backed by explicitly scoped metadata fields."""

    capacity: ListFeasibilityFieldSource
    demand: ListFeasibilityFieldSource

    def __post_init__(self) -> None:
        if self.capacity is None or self.demand is None:
            raise TypeError(
                "CapacityRouteFeasibility requires capacity and demand sources"
            )


ListFeasibilitySource: TypeAlias = (
    EntityCallback | SolutionCallback | CapacityRouteFeasibility
)


@dataclass(frozen=True)
class ListRouteHooks:
    """The complete, independently configured route metadata bundle."""

    depot: ListValueSource
    distance: ListValueSource
    feasible: ListFeasibilitySource

    def __post_init__(self) -> None:
        if self.depot is None or self.distance is None or self.feasible is None:
            raise TypeError(
                "ListRouteHooks requires depot, distance, and feasible sources"
            )


@dataclass(frozen=True)
class ListSavingsHooks:
    """The complete, independently configured Clarke-Wright metadata bundle."""

    depot: ListValueSource
    metric_class: ListValueSource
    distance: ListValueSource
    feasible: ListFeasibilitySource

    def __post_init__(self) -> None:
        if (
            self.depot is None
            or self.metric_class is None
            or self.distance is None
            or self.feasible is None
        ):
            raise TypeError(
                "ListSavingsHooks requires depot, metric_class, distance, and feasible sources"
            )


@dataclass(frozen=True)
class ListMetadata:
    """Canonical declarative metadata for one planning-list variable."""

    route: ListRouteHooks | None = None
    savings: ListSavingsHooks | None = None
    cross_position_distance: ListValueSource | None = None
    intra_position_distance: ListValueSource | None = None


@dataclass(frozen=True)
class FieldMetadata:
    kind: str
    value_range_provider: str | None = None
    candidate_values: Callable[..., object] | None = None
    nearby_value_candidates: Callable[..., object] | None = None
    nearby_value_candidates_field: str | None = None
    nearby_entity_candidates: Callable[..., object] | None = None
    nearby_entity_candidates_field: str | None = None
    nearby_value_distance_meter: Callable[..., object] | None = None
    nearby_value_distance_field: str | None = None
    nearby_entity_distance_meter: Callable[..., object] | None = None
    nearby_entity_distance_field: str | None = None
    allows_unassigned: bool = False
    element_collection: str | None = None
    pinning: bool = False
    element_owner: Callable[..., object] | None = None
    element_owner_field: str | None = None
    construction_element_order_key: Callable[..., object] | None = None
    construction_element_order_field: str | None = None
    precedence_duration: Callable[..., object] | None = None
    precedence_duration_field: str | None = None
    precedence_successors: Callable[..., object] | None = None
    precedence_successors_field: str | None = None
    list_metadata: ListMetadata | None = None


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
    nearby_value_candidates: MetadataSource | None = None,
    nearby_entity_candidates: MetadataSource | None = None,
    nearby_value_distance_meter: MetadataSource | None = None,
    nearby_entity_distance_meter: MetadataSource | None = None,
    allows_unassigned: bool = False,
    pinning: bool = False,
) -> PlanningField:
    value_candidates = _resolve_metadata_source(
        nearby_value_candidates, name="nearby_value_candidates"
    )
    entity_candidates = _resolve_metadata_source(
        nearby_entity_candidates, name="nearby_entity_candidates"
    )
    value_distance = _resolve_metadata_source(
        nearby_value_distance_meter, name="nearby_value_distance_meter"
    )
    entity_distance = _resolve_metadata_source(
        nearby_entity_distance_meter, name="nearby_entity_distance_meter"
    )
    return PlanningField(
        FieldMetadata(
            kind="planning_variable",
            value_range_provider=value_range_provider,
            candidate_values=candidate_values,
            nearby_value_candidates=value_candidates.callback,
            nearby_value_candidates_field=value_candidates.field_name,
            nearby_entity_candidates=entity_candidates.callback,
            nearby_entity_candidates_field=entity_candidates.field_name,
            nearby_value_distance_meter=value_distance.callback,
            nearby_value_distance_field=value_distance.field_name,
            nearby_entity_distance_meter=entity_distance.callback,
            nearby_entity_distance_field=entity_distance.field_name,
            allows_unassigned=allows_unassigned,
            pinning=pinning,
        )
    )


def planning_list_variable(
    *,
    element_collection: str,
    element_owner: MetadataSource | None = None,
    construction_element_order_key: MetadataSource | None = None,
    precedence_duration: MetadataSource | None = None,
    precedence_successors: MetadataSource | None = None,
    route: ListRouteHooks | None = None,
    savings: ListSavingsHooks | None = None,
    cross_position_distance: ListValueSource | None = None,
    intra_position_distance: ListValueSource | None = None,
) -> PlanningField:
    owner = _resolve_metadata_source(
        element_owner,
        name="element_owner",
        field_description="solution-level sequence",
    )
    construction_order = _resolve_metadata_source(
        construction_element_order_key,
        name="construction_element_order_key",
        field_description="solution-level sequence",
    )
    duration = _resolve_metadata_source(
        precedence_duration,
        name="precedence_duration",
        field_description="solution-level sequence",
    )
    successors = _resolve_metadata_source(
        precedence_successors,
        name="precedence_successors",
        field_description="solution-level sequence",
    )
    resolved_route = _resolve_route_hooks(route) if route is not None else None
    resolved_savings = _resolve_savings_hooks(savings) if savings is not None else None
    cross_distance = _resolve_list_value_source(
        cross_position_distance,
        name="cross_position_distance",
        entity_arity=4,
        solution_arity=5,
    )
    intra_distance = _resolve_list_value_source(
        intra_position_distance,
        name="intra_position_distance",
        entity_arity=3,
        solution_arity=4,
    )
    return PlanningField(
        FieldMetadata(
            kind="planning_list_variable",
            element_collection=element_collection,
            element_owner=owner.callback,
            element_owner_field=owner.field_name,
            construction_element_order_key=construction_order.callback,
            construction_element_order_field=construction_order.field_name,
            precedence_duration=duration.callback,
            precedence_duration_field=duration.field_name,
            precedence_successors=successors.callback,
            precedence_successors_field=successors.field_name,
            list_metadata=ListMetadata(
                route=resolved_route,
                savings=resolved_savings,
                cross_position_distance=cross_distance,
                intra_position_distance=intra_distance,
            ),
        )
    )


def _resolve_metadata_source(
    source: MetadataSource | None,
    *,
    name: str,
    field_description: str = "row field",
) -> ResolvedMetadataSource:
    if source is None:
        return ResolvedMetadataSource(callback=None, field_name=None)
    if callable(source):
        return ResolvedMetadataSource(callback=source, field_name=None)
    if isinstance(source, str):
        if source:
            return ResolvedMetadataSource(callback=None, field_name=source)
        raise TypeError(f"{name} field name must not be empty")
    raise TypeError(f"{name} must be a callable or {field_description} name")


def _resolve_route_hooks(route: ListRouteHooks) -> ListRouteHooks:
    if not isinstance(route, ListRouteHooks):
        raise TypeError("route must be a ListRouteHooks instance")
    return ListRouteHooks(
        depot=_resolve_required_list_value_source(
            route.depot,
            name="route.depot",
            entity_arity=1,
            solution_arity=2,
        ),
        distance=_resolve_required_list_value_source(
            route.distance,
            name="route.distance",
            entity_arity=3,
            solution_arity=4,
        ),
        feasible=_resolve_list_feasibility_source(
            route.feasible,
            name="route.feasible",
            entity_arity=2,
            solution_arity=3,
        ),
    )


def _resolve_savings_hooks(savings: ListSavingsHooks) -> ListSavingsHooks:
    if not isinstance(savings, ListSavingsHooks):
        raise TypeError("savings must be a ListSavingsHooks instance")
    return ListSavingsHooks(
        depot=_resolve_required_list_value_source(
            savings.depot,
            name="savings.depot",
            entity_arity=1,
            solution_arity=2,
        ),
        metric_class=_resolve_required_list_value_source(
            savings.metric_class,
            name="savings.metric_class",
            entity_arity=1,
            solution_arity=2,
        ),
        distance=_resolve_required_list_value_source(
            savings.distance,
            name="savings.distance",
            entity_arity=3,
            solution_arity=4,
        ),
        feasible=_resolve_list_feasibility_source(
            savings.feasible,
            name="savings.feasible",
            entity_arity=2,
            solution_arity=3,
        ),
    )


def _resolve_list_value_source(
    source: ListValueSource | None,
    *,
    name: str,
    entity_arity: int,
    solution_arity: int,
) -> ListValueSource | None:
    if source is None:
        return None
    if isinstance(source, (RowField, SolutionField)):
        return source
    if isinstance(source, EntityCallback):
        _validate_scoped_callback(source.callback, name, "EntityCallback", entity_arity)
        return source
    if isinstance(source, SolutionCallback):
        _validate_scoped_callback(
            source.callback,
            name,
            "SolutionCallback",
            solution_arity,
        )
        return source
    raise TypeError(
        f"{name} must be RowField, SolutionField, EntityCallback, or SolutionCallback"
    )


def _resolve_required_list_value_source(
    source: ListValueSource,
    *,
    name: str,
    entity_arity: int,
    solution_arity: int,
) -> ListValueSource:
    resolved = _resolve_list_value_source(
        source,
        name=name,
        entity_arity=entity_arity,
        solution_arity=solution_arity,
    )
    if resolved is None:
        raise TypeError(f"{name} must not be None")
    return resolved


def _resolve_list_feasibility_source(
    source: ListFeasibilitySource,
    *,
    name: str,
    entity_arity: int,
    solution_arity: int,
) -> ListFeasibilitySource:
    if isinstance(source, CapacityRouteFeasibility):
        return CapacityRouteFeasibility(
            capacity=_resolve_list_feasibility_field_source(
                source.capacity,
                name=f"{name}.capacity",
            ),
            demand=_resolve_list_feasibility_field_source(
                source.demand,
                name=f"{name}.demand",
            ),
        )
    if isinstance(source, EntityCallback):
        _validate_scoped_callback(source.callback, name, "EntityCallback", entity_arity)
        return source
    if isinstance(source, SolutionCallback):
        _validate_scoped_callback(
            source.callback,
            name,
            "SolutionCallback",
            solution_arity,
        )
        return source
    raise TypeError(
        f"{name} must be EntityCallback, SolutionCallback, or CapacityRouteFeasibility"
    )


def _resolve_list_feasibility_field_source(
    source: object,
    *,
    name: str,
) -> ListFeasibilityFieldSource:
    if isinstance(source, (RowField, SolutionField)):
        return source
    raise TypeError(f"{name} must be RowField or SolutionField")


def _validate_scoped_callback(
    callback: Callable[..., object],
    name: str,
    scope: str,
    arity: int,
) -> None:
    if _callback_accepts_arity(callback, arity):
        return
    raise TypeError(f"{name} {scope} must accept {arity} positional arguments")


def _callback_accepts_arity(callback: Callable[..., object], arity: int) -> bool:
    try:
        callback_signature = signature(callback)
    except (TypeError, ValueError) as error:
        msg = f"{callback!r} has an introspection-opaque signature"
        raise TypeError(msg) from error

    positional_count = 0
    required_positional_count = 0
    accepts_extra_positionals = False
    for parameter in callback_signature.parameters.values():
        if parameter.kind in (
            Parameter.POSITIONAL_ONLY,
            Parameter.POSITIONAL_OR_KEYWORD,
        ):
            positional_count += 1
            if parameter.default is Parameter.empty:
                required_positional_count += 1
        elif parameter.kind is Parameter.VAR_POSITIONAL:
            accepts_extra_positionals = True
        elif (
            parameter.kind is Parameter.KEYWORD_ONLY
            and parameter.default is Parameter.empty
        ):
            return False

    if arity < required_positional_count:
        return False
    return accepts_extra_positionals or arity <= positional_count
