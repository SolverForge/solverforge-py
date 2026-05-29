from __future__ import annotations

from dataclasses import dataclass
from typing import ClassVar, Iterable, Self


@dataclass(frozen=True, order=True)
class SoftScore:
    soft: int = 0

    ZERO: ClassVar[SoftScore]
    ONE_SOFT: ClassVar[SoftScore]

    @classmethod
    def of(cls, soft: int) -> Self:
        return cls(soft)

    def to_native(self) -> dict[str, object]:
        return {"family": "soft", "levels": [self.soft]}


SoftScore.ZERO = SoftScore(0)
SoftScore.ONE_SOFT = SoftScore(1)


@dataclass(frozen=True, order=True)
class HardSoftScore:
    hard: int = 0
    soft: int = 0

    ZERO: ClassVar[HardSoftScore]
    ONE_HARD: ClassVar[HardSoftScore]
    ONE_SOFT: ClassVar[HardSoftScore]

    @classmethod
    def of(cls, hard: int, soft: int) -> Self:
        return cls(hard, soft)

    @classmethod
    def of_hard(cls, hard: int) -> Self:
        return cls(hard, 0)

    @classmethod
    def of_soft(cls, soft: int) -> Self:
        return cls(0, soft)

    def to_native(self) -> dict[str, object]:
        return {"family": "hard_soft", "levels": [self.hard, self.soft]}


HardSoftScore.ZERO = HardSoftScore(0, 0)
HardSoftScore.ONE_HARD = HardSoftScore(1, 0)
HardSoftScore.ONE_SOFT = HardSoftScore(0, 1)


@dataclass(frozen=True, order=True)
class HardSoftDecimalScore:
    hard_scaled: int = 0
    soft_scaled: int = 0

    SCALE: ClassVar[int] = 100_000
    ZERO: ClassVar[HardSoftDecimalScore]
    ONE_HARD: ClassVar[HardSoftDecimalScore]
    ONE_SOFT: ClassVar[HardSoftDecimalScore]

    @classmethod
    def of(cls, hard: int, soft: int) -> Self:
        return cls(hard * cls.SCALE, soft * cls.SCALE)

    @classmethod
    def of_hard(cls, hard: int) -> Self:
        return cls(hard * cls.SCALE, 0)

    @classmethod
    def of_soft(cls, soft: int) -> Self:
        return cls(0, soft * cls.SCALE)

    @classmethod
    def of_hard_scaled(cls, hard: int) -> Self:
        return cls(hard, 0)

    @classmethod
    def of_soft_scaled(cls, soft: int) -> Self:
        return cls(0, soft)

    @classmethod
    def one_hard(cls) -> Self:
        return cls(1, 0)

    @classmethod
    def one_soft(cls) -> Self:
        return cls(0, 1)

    def to_native(self) -> dict[str, object]:
        return {
            "family": "hard_soft_decimal",
            "levels": [self.hard_scaled, self.soft_scaled],
        }


HardSoftDecimalScore.ZERO = HardSoftDecimalScore(0, 0)
HardSoftDecimalScore.ONE_HARD = HardSoftDecimalScore(HardSoftDecimalScore.SCALE, 0)
HardSoftDecimalScore.ONE_SOFT = HardSoftDecimalScore(0, HardSoftDecimalScore.SCALE)


@dataclass(frozen=True, order=True)
class HardMediumSoftScore:
    hard: int = 0
    medium: int = 0
    soft: int = 0

    ZERO: ClassVar[HardMediumSoftScore]
    ONE_HARD: ClassVar[HardMediumSoftScore]
    ONE_MEDIUM: ClassVar[HardMediumSoftScore]
    ONE_SOFT: ClassVar[HardMediumSoftScore]

    @classmethod
    def of(cls, hard: int, medium: int, soft: int) -> Self:
        return cls(hard, medium, soft)

    def to_native(self) -> dict[str, object]:
        return {
            "family": "hard_medium_soft",
            "levels": [self.hard, self.medium, self.soft],
        }


HardMediumSoftScore.ZERO = HardMediumSoftScore(0, 0, 0)
HardMediumSoftScore.ONE_HARD = HardMediumSoftScore(1, 0, 0)
HardMediumSoftScore.ONE_MEDIUM = HardMediumSoftScore(0, 1, 0)
HardMediumSoftScore.ONE_SOFT = HardMediumSoftScore(0, 0, 1)


def score_family(score_type: type[object]) -> str:
    if score_type is SoftScore:
        return "soft"
    if score_type is HardSoftScore:
        return "hard_soft"
    if score_type is HardSoftDecimalScore:
        return "hard_soft_decimal"
    if score_type is HardMediumSoftScore:
        return "hard_medium_soft"
    msg = f"unsupported score type {score_type!r}"
    raise TypeError(msg)


def score_to_native(value: object, score_family: str | None = None) -> dict[str, object]:
    if isinstance(value, int):
        return {"family": "soft", "levels": [value]}
    if hasattr(value, "to_native"):
        native = value.to_native()
        if isinstance(native, dict):
            return native
    if isinstance(value, Iterable):
        levels = [int(item) for item in value]
        return {"family": _sequence_score_family(levels, score_family), "levels": levels}
    msg = f"cannot convert {value!r} to SolverForge score"
    raise TypeError(msg)


def _sequence_score_family(levels: list[int], score_family: str | None) -> str:
    if len(levels) == 1:
        return "soft"
    if len(levels) == 2:
        if score_family == "hard_soft_decimal":
            return "hard_soft_decimal"
        return "hard_soft"
    if len(levels) == 3:
        return "hard_medium_soft"
    msg = f"unsupported dynamic score level count {len(levels)}"
    raise TypeError(msg)
