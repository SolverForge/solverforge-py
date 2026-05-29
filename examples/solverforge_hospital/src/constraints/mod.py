from __future__ import annotations

from solverforge import ConstraintFactory, constraint_provider

from . import (
    assigned_shift,
    balance_assignments,
    desired_day,
    minimum_rest,
    one_shift_per_day,
    overlapping_shift,
    required_skill,
    unavailable_employee,
    undesired_day,
)

CONSTRAINTS = [
    {"name": "Assigned shift", "type": "hard"},
    {"name": "Required skill", "type": "hard"},
    {"name": "Overlapping shift", "type": "hard"},
    {"name": "At least 10 hours between 2 shifts", "type": "hard"},
    {"name": "One shift per day", "type": "hard"},
    {"name": "Unavailable employee", "type": "hard"},
    {"name": "Undesired day for employee", "type": "soft"},
    {"name": "Desired day for employee", "type": "soft"},
    {"name": "Balance employee assignments", "type": "soft"},
]


@constraint_provider
def hospital_constraints(factory: ConstraintFactory) -> list[object]:
    return [
        assigned_shift.build(factory),
        required_skill.build(factory),
        overlapping_shift.build(factory),
        minimum_rest.build(factory),
        one_shift_per_day.build(factory),
        unavailable_employee.build(factory),
        undesired_day.build(factory),
        desired_day.build(factory),
        balance_assignments.build(factory),
    ]
