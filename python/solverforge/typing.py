from __future__ import annotations

from collections.abc import Callable, Sequence
from typing import Protocol, TypeAlias

ScoreLike: TypeAlias = int | Sequence[int]
FilterCallback: TypeAlias = Callable[..., bool]
WeightCallback: TypeAlias = Callable[..., ScoreLike]


class PlanningEntityProtocol(Protocol):
    pass


class PlanningSolutionProtocol(Protocol):
    score: object | None

