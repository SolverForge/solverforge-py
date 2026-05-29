from __future__ import annotations

from solverforge import ConstraintFactory, joiner

from ..domain.plan import (
    SCORE_SCALE,
    STRUCTURAL_MINUTE_HARD_UNITS,
    Shift,
    assigned,
    hard_scaled,
    overlap_minutes,
    overlaps,
)


def build(factory: ConstraintFactory) -> object:
    return (
        factory.for_each(Shift)
        .filter(assigned)
        .join(joiner.equal(lambda shift: shift.employee_idx))
        .filter(lambda left, right: left.index < right.index and overlaps(left, right))
        .penalize(
            lambda left, right: hard_scaled(
                overlap_minutes(left, right) * STRUCTURAL_MINUTE_HARD_UNITS * SCORE_SCALE
            )
        )
        .named("Overlapping shift")
    )
