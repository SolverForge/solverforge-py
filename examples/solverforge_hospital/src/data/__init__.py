from __future__ import annotations

from enum import StrEnum

from ..domain import HospitalPlan
from .data_seed import demo_plan


class DemoData(StrEnum):
    LARGE = "LARGE"


def list_demo_data() -> list[str]:
    return [DemoData.LARGE.value]


def generate(demo: DemoData | str) -> HospitalPlan:
    if DemoData(str(demo)) is DemoData.LARGE:
        return demo_plan()
    raise ValueError(str(demo))


__all__ = ["DemoData", "generate", "list_demo_data"]
