from __future__ import annotations

import math
from datetime import date, datetime, time, timedelta
from typing import Any

from solverforge import (
    HardSoftDecimalScore,
    planning_entity,
    planning_solution,
    planning_variable,
)

from .care_hub import CareHub, care_hub_distance, care_hub_from_skill
from .employee import Employee

SCORE_SCALE = 100_000
STRUCTURAL_MINUTE_HARD_UNITS = 20
BASE_DATE = date(2024, 1, 1)


def hard_scaled(value: int) -> HardSoftDecimalScore:
    return HardSoftDecimalScore.of_hard_scaled(value)


def nearby_shift_distance(left: Any, right: Any) -> float:
    return shift_to_shift_nearby_distance(left, right)


def eligible_employee_candidates(shift: Any) -> list[int]:
    return [
        employee_idx
        for employee_idx, (has_skill, unavailable_minutes) in enumerate(
            zip(
                shift.employee_has_skill,
                shift.employee_unavailable_minutes,
                strict=True,
            )
        )
        if has_skill and unavailable_minutes == 0
    ]


@planning_entity
class Shift:
    employee_idx = planning_variable(
        value_range_provider="employee_indices",
        candidate_values=eligible_employee_candidates,
        nearby_value_candidates="employee_nearby_candidates",
        nearby_entity_candidates="shift_nearby_candidates",
        nearby_value_distance_meter="employee_nearby_distance",
        nearby_entity_distance_meter=nearby_shift_distance,
        allows_unassigned=True,
    )

    def __init__(
        self,
        *,
        id: str,
        index: int,
        start: str,
        end: str,
        location: str,
        care_hub: CareHub,
        required_skill: str,
        employees: list[Employee],
        employee_idx: int | None = None,
    ) -> None:
        self.id = id
        self.index = index
        self.start = start
        self.end = end
        self.start_dt = parse_datetime(start)
        self.end_dt = parse_datetime(end)
        self.start_minute = minute_index(self.start_dt)
        self.end_minute = minute_index(self.end_dt)
        self.location = location
        self.care_hub = care_hub
        self.required_skill = required_skill
        self.touched_dates = tuple(
            day.isoformat() for day in dates_touched_by_span(self.start_dt, self.end_dt)
        )
        self.employee_has_skill = [
            required_skill in employee.skills for employee in employees
        ]
        self.employee_unavailable_minutes = [
            sum(
                overlap_minutes_for_day(self.start_dt, self.end_dt, parse_date(day))
                for day in employee.unavailable_days
            )
            for employee in employees
        ]
        self.employee_undesired_day_count = [
            sum(1 for day in self.touched_dates if day in employee.undesired_days)
            for employee in employees
        ]
        self.employee_desired_day_count = [
            sum(1 for day in self.touched_dates if day in employee.desired_days)
            for employee in employees
        ]
        self.employee_undesired_day = [
            count > 0 for count in self.employee_undesired_day_count
        ]
        self.employee_desired_day = [
            count > 0 for count in self.employee_desired_day_count
        ]
        self.employee_nearby_distance = [
            shift_to_employee_nearby_distance(self, employee) for employee in employees
        ]
        self.employee_nearby_candidates = eligible_employee_candidates(self)
        self.shift_nearby_candidates: list[int] = []
        self.employee_idx = validate_employee_idx(employee_idx, len(employees))

    @property
    def duration_minutes(self) -> int:
        return self.end_minute - self.start_minute


def assigned(shift: Any) -> bool:
    return shift.employee_idx is not None


def validate_employee_idx(employee_idx: int | None, employee_count: int) -> int | None:
    if employee_idx is None:
        return None
    if type(employee_idx) is not int:
        msg = f"employee_idx must be an integer or None, got {employee_idx!r}"
        raise TypeError(msg)
    if not 0 <= employee_idx < employee_count:
        msg = f"employee_idx {employee_idx} is outside 0..{employee_count - 1}"
        raise ValueError(msg)
    return employee_idx


def same_employee(left: Any, right: Any) -> bool:
    return assigned(left) and left.employee_idx == right.employee_idx


def lacks_required_skill(shift: Any) -> bool:
    return assigned(shift) and not shift.employee_has_skill[shift.employee_idx]


def unavailable_minutes(shift: Any) -> int:
    if not assigned(shift):
        return 0
    return int(shift.employee_unavailable_minutes[shift.employee_idx])


def employee_unavailable_minutes(shift: Any, employee: Any) -> int:
    shift_start = parse_datetime(shift.start)
    shift_end = parse_datetime(shift.end)
    return sum(
        overlap_minutes_for_day(shift_start, shift_end, parse_date(day))
        for day in employee.unavailable_days
    )


def overlaps(left: Any, right: Any) -> bool:
    return bool(
        left.start_minute < right.end_minute and right.start_minute < left.end_minute
    )


def overlap_minutes(left: Any, right: Any) -> int:
    overlap_start = max(left.start_minute, right.start_minute)
    overlap_end = min(left.end_minute, right.end_minute)
    return int(max(0, overlap_end - overlap_start))


def gap_minutes(left: Any, right: Any) -> int | None:
    if left.end_minute <= right.start_minute:
        return int(right.start_minute - left.end_minute)
    if right.end_minute <= left.start_minute:
        return int(left.start_minute - right.end_minute)
    return None


def same_day(left: Any, right: Any) -> bool:
    return any(day in right.touched_dates for day in left.touched_dates)


def undesired_day(shift: Any) -> bool:
    return assigned(shift) and shift.employee_undesired_day[shift.employee_idx]


def desired_day(shift: Any) -> bool:
    return assigned(shift) and shift.employee_desired_day[shift.employee_idx]


def employee_undesired_day_count(shift: Any, employee: Any) -> int:
    return sum(1 for day in shift.touched_dates if day in employee.undesired_days)


def employee_desired_day_count(shift: Any, employee: Any) -> int:
    return sum(1 for day in shift.touched_dates if day in employee.desired_days)


def parse_datetime(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00")).replace(tzinfo=None)


def parse_date(value: str) -> date:
    return date.fromisoformat(value)


def minute_index(value: datetime) -> int:
    return int((value - datetime.combine(BASE_DATE, time())).total_seconds() // 60)


def day_to_iso(day: int) -> str:
    return (BASE_DATE + timedelta(days=day)).isoformat()


def iso_to_day(value: str) -> int:
    return (parse_date(value) - BASE_DATE).days


def dates_touched_by_span(start: datetime, end: datetime) -> list[date]:
    touched: list[date] = []
    current = start.date()
    while current <= end.date():
        if overlap_minutes_for_day(start, end, current) > 0:
            touched.append(current)
        current += timedelta(days=1)
    return touched


def overlap_minutes_for_day(start: datetime, end: datetime, target: date) -> int:
    day_start = datetime.combine(target, time())
    day_end = day_start + timedelta(days=1)
    overlap_start = max(start, day_start)
    overlap_end = min(end, day_end)
    if overlap_start < overlap_end:
        return int((overlap_end - overlap_start).total_seconds() // 60)
    return 0


def start_band_distance(left_hour: int, right_hour: int) -> float:
    return float(
        min(abs(start_band_index(left_hour) - start_band_index(right_hour)), 2)
    )


def start_band_index(hour: int) -> int:
    if hour <= 7:
        return 0
    if hour <= 12:
        return 1
    if hour <= 17:
        return 2
    return 3


def shift_to_employee_nearby_distance(shift: Shift, employee: Employee) -> float:
    distance = 10.0 * care_hub_distance(shift.care_hub, employee.home_hub)
    if shift.required_skill not in employee.skills:
        distance += 10_000.0
    elif care_hub_from_skill(shift.required_skill) != employee.home_hub:
        distance += 12.0
    if any(day in employee.unavailable_dates for day in shift.touched_dates):
        distance += 2_000.0
    return distance


def shift_to_shift_nearby_distance(left: Any, right: Any) -> float:
    return 10.0 * care_hub_distance(
        left.care_hub, right.care_hub
    ) + start_band_distance(
        parse_datetime(left.start).hour,
        parse_datetime(right.start).hour,
    )


def balance_score(shifts: list[Shift]) -> HardSoftDecimalScore:
    counts: dict[int, int] = {}
    for shift in shifts:
        if shift.employee_idx is not None:
            counts[shift.employee_idx] = counts.get(shift.employee_idx, 0) + 1
    if not counts:
        return HardSoftDecimalScore.ZERO
    total = sum(counts.values())
    mean = total / len(counts)
    variance = sum((count - mean) ** 2 for count in counts.values()) / len(counts)
    return HardSoftDecimalScore.of_soft_scaled(round(math.sqrt(variance)))


from ..constraints.mod import hospital_constraints  # noqa: E402


@planning_solution(score=HardSoftDecimalScore, constraints=hospital_constraints)
class HospitalPlan:
    shifts: list[Shift]

    def __init__(self, employees: list[Employee], shifts: list[Shift]) -> None:
        self.employees = employees
        self.shifts = shifts
        self.employee_indices = list(range(len(employees)))
        shift_indices = list(range(len(shifts)))
        for shift in shifts:
            shift.shift_nearby_candidates = shift_indices
        self.score = None
