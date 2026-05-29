from __future__ import annotations

from solverforge import ConstraintFactory, joiner

from ..domain.employee import Employee
from ..domain.plan import SCORE_SCALE, Shift, assigned, hard_scaled


def build(factory: ConstraintFactory) -> object:
    return (
        factory.for_each(Shift)
        .filter(assigned)
        .join(
            Employee,
            joiner.equal_bi(lambda shift: shift.employee_idx, lambda employee: employee.index),
        )
        .filter(lambda shift, employee: not shift.employee_has_skill[employee.index])
        .penalize(hard_scaled(10 * SCORE_SCALE))
        .named("Required skill")
    )
