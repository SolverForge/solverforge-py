from __future__ import annotations

from dataclasses import dataclass

from solverforge import problem_fact

from .care_hub import CareHub


@problem_fact
@dataclass(frozen=True)
class Employee:
    index: int
    id: str
    name: str
    home_hub: CareHub
    skills: tuple[str, ...]
    unavailable_dates: tuple[str, ...] = ()
    undesired_dates: tuple[str, ...] = ()
    desired_dates: tuple[str, ...] = ()

    @property
    def unavailable_days(self) -> tuple[str, ...]:
        return self.unavailable_dates

    @property
    def undesired_days(self) -> tuple[str, ...]:
        return self.undesired_dates

    @property
    def desired_days(self) -> tuple[str, ...]:
        return self.desired_dates
