from __future__ import annotations

from typing import Any, cast

from solverforge import ConstraintFactory, HardSoftDecimalScore

from ..domain.plan import Shift


def employee_idx(shift: Any) -> int | None:
    return cast(int | None, shift.employee_idx)


def build(factory: ConstraintFactory) -> object:
    return (
        factory.for_each(Shift)
        .balance(employee_idx)
        .penalize(HardSoftDecimalScore.one_soft())
        .named("Balance employee assignments")
    )
