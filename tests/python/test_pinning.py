"""Entity pinning: preserved input state and its declaration contract.

A pinned entity keeps its input planning state — the genuine scalar value it
starts with, and the elements its list variable owns — while every unpinned
entity in the same solve is still constructed and searched. The declaration is
a per-instance ``bool``; the enforcement is the compiled SolverForge runtime,
which never edits a pinned row.
"""

from __future__ import annotations

import pytest

from solverforge import (
    ConstraintFactory,
    HardSoftScore,
    ModelValidationError,
    SoftScore,
    Solver,
    SolverManager,
    constraint_provider,
    planning_entity,
    planning_list_variable,
    planning_pin,
    planning_solution,
    planning_variable,
    problem_fact,
)
from solverforge import _native
from solverforge.model import build_schema

STEP_LIMITS = {"termination": {"step_count_limit": 200}}


@planning_entity
class Shift:
    pinned = planning_pin()
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self, pinned: bool, nurse: int | None = None) -> None:
        self.pinned = pinned
        self.nurse = nurse


@constraint_provider
def shift_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(Shift)
        .filter(lambda shift: shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("unassigned"),
        # Choosing nurse 1 is strictly worse, so an unpinned shift leaves it.
        factory.for_each(Shift)
        .filter(lambda shift: shift.nurse == 1)
        .penalize(SoftScore.of(10))
        .named("expensive nurse"),
    ]


@planning_solution(score=HardSoftScore, constraints=shift_constraints)
class Schedule:
    shifts: list[Shift]

    def __init__(self, shifts: list[Shift]) -> None:
        self.shifts = shifts
        self.nurses = [0, 1]
        self.score = None


@planning_entity
class FreeShift:
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self, nurse: int | None = None) -> None:
        self.nurse = nurse


@constraint_provider
def free_shift_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(FreeShift)
        .filter(lambda shift: shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("unassigned"),
        factory.for_each(FreeShift)
        .filter(lambda shift: shift.nurse == 1)
        .penalize(SoftScore.of(10))
        .named("expensive nurse"),
    ]


@planning_solution(score=HardSoftScore, constraints=free_shift_constraints)
class FreeSchedule:
    shifts: list[FreeShift]

    def __init__(self, shifts: list[FreeShift]) -> None:
        self.shifts = shifts
        self.nurses = [0, 1]
        self.score = None


@planning_entity
class Vehicle:
    pinned = planning_pin()
    visits = planning_list_variable(element_collection="visit_values")

    def __init__(self, pinned: bool, visits: list[int] | None = None) -> None:
        self.pinned = pinned
        self.visits: list[int] = list(visits or [])


@constraint_provider
def route_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(Vehicle)
        .filter(lambda vehicle: len(vehicle.visits) > 1)
        .penalize(SoftScore.of(1))
        .named("long route"),
    ]


@planning_solution(score=SoftScore, constraints=route_constraints)
class Routes:
    vehicles: list[Vehicle]

    def __init__(self, vehicles: list[Vehicle]) -> None:
        self.vehicles = vehicles
        self.visit_values = [0, 1, 2, 3]
        self.score = None


@planning_entity
class FreeVehicle:
    visits = planning_list_variable(element_collection="visit_values")

    def __init__(self, visits: list[int] | None = None) -> None:
        self.visits: list[int] = list(visits or [])


@constraint_provider
def free_route_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(FreeVehicle)
        .filter(lambda vehicle: len(vehicle.visits) > 1)
        .penalize(SoftScore.of(1))
        .named("long route"),
    ]


@planning_solution(score=SoftScore, constraints=free_route_constraints)
class FreeRoutes:
    vehicles: list[FreeVehicle]

    def __init__(self, vehicles: list[FreeVehicle]) -> None:
        self.vehicles = vehicles
        self.visit_values = [0, 1, 2, 3]
        self.score = None


@planning_entity
class RequiredShift:
    pinned = planning_pin()
    nurse = planning_variable(value_range_provider="nurses")

    def __init__(self, pinned: bool) -> None:
        self.pinned = pinned
        self.nurse: int | None = None


@constraint_provider
def required_constraints(factory: ConstraintFactory):
    return []


@planning_solution(score=HardSoftScore, constraints=required_constraints)
class RequiredSchedule:
    shifts: list[RequiredShift]

    def __init__(self) -> None:
        self.shifts = [RequiredShift(pinned=True)]
        self.nurses = [0, 1]
        self.score = None


@planning_entity
class OptionalShift:
    pinned = planning_pin()
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self, pinned: bool) -> None:
        self.pinned = pinned
        self.nurse: int | None = None


@constraint_provider
def optional_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(OptionalShift)
        .filter(lambda shift: shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("unassigned"),
    ]


@planning_solution(score=HardSoftScore, constraints=optional_constraints)
class OptionalSchedule:
    shifts: list[OptionalShift]

    def __init__(self) -> None:
        self.shifts = [OptionalShift(pinned=True), OptionalShift(pinned=False)]
        self.nurses = [0, 1]
        self.score = None


@planning_entity
class UnsetPin:
    pinned = planning_pin()
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self) -> None:
        self.nurse = None


@constraint_provider
def unset_pin_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(UnsetPin)
        .filter(lambda shift: shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("unassigned"),
    ]


@planning_solution(score=HardSoftScore, constraints=unset_pin_constraints)
class UnsetPinSchedule:
    shifts: list[UnsetPin]

    def __init__(self) -> None:
        self.shifts = [UnsetPin()]
        self.nurses = [0, 1]
        self.score = None


@planning_entity
class IntPin:
    pinned = planning_pin()
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self) -> None:
        self.pinned = 1
        self.nurse = None


@constraint_provider
def int_pin_constraints(factory: ConstraintFactory):
    return [
        factory.for_each(IntPin)
        .filter(lambda shift: shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("unassigned"),
    ]


@planning_solution(score=HardSoftScore, constraints=int_pin_constraints)
class IntPinSchedule:
    shifts: list[IntPin]

    def __init__(self) -> None:
        self.shifts = [IntPin()]
        self.nurses = [0, 1]
        self.score = None


def test_schema_declares_the_entity_pin_field() -> None:
    schema = build_schema(Schedule([Shift(pinned=True)]))
    fields = schema["entities"][0]["fields"]

    assert [field["kind"] for field in fields] == ["planning_pin", "planning_variable"]
    assert fields[0]["name"] == "pinned"

    free_schema = build_schema(FreeSchedule([FreeShift()]))
    assert [field["kind"] for field in free_schema["entities"][0]["fields"]] == [
        "planning_variable"
    ]


def test_declared_pin_field_is_compiled_into_the_schema() -> None:
    _native.validate_schema(build_schema(Schedule([Shift(pinned=True)])))
    _native.validate_schema(build_schema(FreeSchedule([FreeShift()])))


def test_two_pin_fields_on_one_entity_are_rejected_at_declaration() -> None:
    with pytest.raises(ModelValidationError, match="more than one planning_pin"):

        @planning_entity
        class DoublyPinned:
            first = planning_pin()
            second = planning_pin()
            nurse = planning_variable(value_range_provider="nurses")

            def __init__(self) -> None:
                self.nurse = None


def test_a_problem_fact_cannot_declare_a_pin_field() -> None:
    with pytest.raises(ModelValidationError, match="only valid on a planning entity"):

        @problem_fact
        class PinnedFact:
            pinned = planning_pin()


def test_a_hand_built_schema_cannot_declare_two_pin_fields() -> None:
    # `build_schema` aliases the class's own field list, so a caller that mutates
    # a schema copies the containers first.
    schema = build_schema(Schedule([Shift(pinned=True)]))
    entity = dict(schema["entities"][0])
    fields = [dict(field) for field in entity["fields"]]
    fields.append({"name": "second_pin", "kind": "planning_pin"})
    entity["fields"] = fields
    mutated = dict(schema)
    mutated["entities"] = [entity]

    with pytest.raises(RuntimeError, match="more than one planning_pin"):
        _native.validate_schema(mutated)

    # The declaration itself is untouched.
    assert [
        field["kind"] for field in build_schema(Schedule([]))["entities"][0]["fields"]
    ] == [
        "planning_pin",
        "planning_variable",
    ]


def test_a_hand_built_schema_cannot_name_one_field_as_variable_and_pin() -> None:
    # Only the pin field's name changes: adding a second pin field would trip the
    # duplicate-pin check first and never reach the name-collision invariant.
    schema = build_schema(Schedule([Shift(pinned=True)]))
    entity = dict(schema["entities"][0])
    fields = [dict(field) for field in entity["fields"]]
    pin_index = next(
        index for index, field in enumerate(fields) if field["kind"] == "planning_pin"
    )
    fields[pin_index]["name"] = "nurse"
    entity["fields"] = fields
    mutated = dict(schema)
    mutated["entities"] = [entity]

    with pytest.raises(
        RuntimeError, match="both a planning variable and a planning_pin"
    ):
        _native.validate_schema(mutated)


def test_pinned_entity_keeps_its_input_value() -> None:
    solved = Solver.solve(
        Schedule([Shift(pinned=True, nurse=1), Shift(pinned=False)]), STEP_LIMITS
    )

    # The pinned row keeps the expensive input value; the free row takes the
    # cheapest one, so the pinned solve ends one soft step away from optimal.
    assert [shift.nurse for shift in solved.shifts] == [1, 0]
    assert solved.score["levels"] == [0, -10]


def test_unpinned_control_abandons_the_expensive_value() -> None:
    solved = Solver.solve(FreeSchedule([FreeShift(nurse=1), FreeShift()]), STEP_LIMITS)

    # Same model without the pin declaration: the expensive input is abandoned.
    assert [shift.nurse for shift in solved.shifts] == [0, 0]
    assert solved.score["levels"] == [0, 0]


def test_pinned_list_owner_keeps_its_route_and_free_owner_constructs() -> None:
    solved = Solver.solve(
        Routes([Vehicle(pinned=True, visits=[0, 1]), Vehicle(pinned=False)]),
        STEP_LIMITS,
    )

    assert solved.vehicles[0].visits == [0, 1]
    assert sorted(solved.vehicles[1].visits) == [2, 3]
    owned = [element for vehicle in solved.vehicles for element in vehicle.visits]
    assert sorted(owned) == [0, 1, 2, 3]


def test_unpinned_control_route_is_rebuilt() -> None:
    solved = Solver.solve(
        FreeRoutes([FreeVehicle(visits=[0, 1]), FreeVehicle()]), STEP_LIMITS
    )

    # Without the pin declaration the first route is no longer the input route.
    assert solved.vehicles[0].visits != [0, 1]


def test_pinned_required_scalar_left_unassigned_fails_completion() -> None:
    # Pinning preserves input state; it does not exempt a required row from the
    # mandatory-completion gate when nothing assigned it.
    with pytest.raises(RuntimeError, match="mandatory planning work incomplete"):
        Solver.solve(RequiredSchedule(), STEP_LIMITS)


def test_pinned_optional_scalar_stays_unassigned() -> None:
    solved = Solver.solve(OptionalSchedule(), STEP_LIMITS)

    # The pinned optional row keeps its unassigned input even though assigning it
    # would remove a hard penalty; the free row is assigned.
    assert [shift.nurse for shift in solved.shifts] == [None, 0]
    assert solved.score["levels"] == [-1, 0]


def test_retained_solve_preserves_pinned_state() -> None:
    manager = SolverManager()
    handle = manager.solve(
        Routes([Vehicle(pinned=True, visits=[0, 1]), Vehicle(pinned=False)])
    )
    status = manager.wait(handle.job_id)
    snapshot = manager.snapshot(handle.job_id)

    assert status["lifecycle_state"] == "COMPLETED"
    assert snapshot.vehicles[0].visits == [0, 1]
    assert sorted(snapshot.vehicles[1].visits) == [2, 3]
    manager.delete(handle.job_id)


def test_pinned_entity_without_its_declared_bool_is_rejected() -> None:
    with pytest.raises(RuntimeError, match="must set a bool `pinned` attribute"):
        Solver.solve(UnsetPinSchedule(), STEP_LIMITS)

    with pytest.raises(RuntimeError, match="attribute `pinned` must be a bool"):
        Solver.solve(IntPinSchedule(), STEP_LIMITS)
