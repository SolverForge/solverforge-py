from __future__ import annotations

from solverforge import ConstraintFactory, joiner

from ..domain.plan import (
    SCORE_SCALE,
    STRUCTURAL_MINUTE_HARD_UNITS,
    Shift,
    assigned,
    gap_minutes,
    hard_scaled,
)


def build(factory: ConstraintFactory) -> object:
    return (
        factory.for_each(Shift)
        .filter(assigned)
        .join(joiner.equal(lambda shift: shift.employee_idx))
        .filter(
            lambda left, right: left.index < right.index
            and (gap := gap_minutes(left, right)) is not None
            and 0 <= gap < 600
        )
        .penalize(
            lambda left, right: hard_scaled(
                (600 - (gap_minutes(left, right) or 0))
                * STRUCTURAL_MINUTE_HARD_UNITS
                * SCORE_SCALE
            )
        )
        .named("At least 10 hours between 2 shifts")
    )
