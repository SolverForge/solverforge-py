from __future__ import annotations

from solverforge import ConstraintFactory

from ..domain.plan import SCORE_SCALE, Shift, hard_scaled


def build(factory: ConstraintFactory) -> object:
    return (
        factory.for_each(Shift)
        .filter(lambda shift: shift.employee_idx is None)
        .penalize(hard_scaled(SCORE_SCALE))
        .named("Assigned shift")
    )
