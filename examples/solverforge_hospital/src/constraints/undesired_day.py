from __future__ import annotations

from solverforge import ConstraintFactory, HardSoftDecimalScore, joiner

from ..domain.employee import Employee
from ..domain.plan import Shift, assigned


def build(factory: ConstraintFactory) -> object:
    return (
        factory.for_each(Shift)
        .filter(assigned)
        .join(
            Employee,
            joiner.equal_bi(lambda shift: shift.employee_idx, lambda employee: employee.index),
        )
        .filter(lambda shift, employee: shift.employee_undesired_day_count[employee.index] > 0)
        .penalize(
            lambda shift, employee: HardSoftDecimalScore.of_soft(
                shift.employee_undesired_day_count[employee.index]
            )
        )
        .named("Undesired day for employee")
    )
