from __future__ import annotations

from solverforge import ConstraintFactory, joiner

from ..domain.employee import Employee
from ..domain.plan import (
    SCORE_SCALE,
    STRUCTURAL_MINUTE_HARD_UNITS,
    Shift,
    assigned,
    hard_scaled,
)


def build(factory: ConstraintFactory) -> object:
    return (
        factory.for_each(Shift)
        .filter(assigned)
        .join(
            Employee,
            joiner.equal_bi(lambda shift: shift.employee_idx, lambda employee: employee.index),
        )
        .filter(lambda shift, employee: shift.employee_unavailable_minutes[employee.index] > 0)
        .penalize(
            lambda shift, employee: hard_scaled(
                shift.employee_unavailable_minutes[employee.index]
                * STRUCTURAL_MINUTE_HARD_UNITS
                * SCORE_SCALE
            )
        )
        .named("Unavailable employee")
    )
