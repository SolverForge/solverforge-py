from __future__ import annotations

from solverforge import ConstraintFactory, joiner

from ..domain.plan import SCORE_SCALE, Shift, assigned, hard_scaled, same_day


def build(factory: ConstraintFactory) -> object:
    return (
        factory.for_each(Shift)
        .filter(assigned)
        .join(joiner.equal(lambda shift: shift.employee_idx))
        .filter(lambda left, right: left.index < right.index and same_day(left, right))
        .penalize(hard_scaled(20 * SCORE_SCALE))
        .named("One shift per day")
    )
