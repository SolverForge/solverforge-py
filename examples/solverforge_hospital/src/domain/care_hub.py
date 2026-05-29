from __future__ import annotations

from enum import StrEnum


class CareHub(StrEnum):
    AMBULATORY = "ambulatory"
    NEUROLOGY = "neurology"
    CRITICAL_CARE = "critical_care"
    PEDIATRIC_CARE = "pediatric_care"
    SURGERY = "surgery"
    RADIOLOGY = "radiology"
    OUTPATIENT = "outpatient"
    UNKNOWN = "unknown"


def care_hub_from_location(location: str) -> CareHub:
    return {
        "Ambulatory care": CareHub.AMBULATORY,
        "Neurology": CareHub.NEUROLOGY,
        "Critical care": CareHub.CRITICAL_CARE,
        "Pediatric care": CareHub.PEDIATRIC_CARE,
        "Surgery": CareHub.SURGERY,
        "Radiology": CareHub.RADIOLOGY,
        "Outpatient": CareHub.OUTPATIENT,
    }.get(location, CareHub.UNKNOWN)


def care_hub_from_skill(skill: str) -> CareHub | None:
    if skill in {"Ambulatory doctor", "Ambulatory nurse"}:
        return CareHub.AMBULATORY
    if skill in {"Neurology doctor", "Neurology nurse", "Cardiology"}:
        return CareHub.NEUROLOGY
    if skill in {"Critical care doctor", "Critical care nurse"}:
        return CareHub.CRITICAL_CARE
    if skill in {"Pediatric doctor", "Pediatric nurse"}:
        return CareHub.PEDIATRIC_CARE
    if skill in {"Surgery doctor", "Surgery nurse", "Anaesthetics"}:
        return CareHub.SURGERY
    if skill in {"Radiology day", "Radiology nurse", "Radiology call"}:
        return CareHub.RADIOLOGY
    if skill in {"Outpatient doctor", "Outpatient nurse"}:
        return CareHub.OUTPATIENT
    return None


def care_hub_distance(left: CareHub, right: CareHub) -> float:
    lx, ly = care_hub_position(left)
    rx, ry = care_hub_position(right)
    return float(abs(lx - rx) + abs(ly - ry))


def care_hub_position(hub: CareHub) -> tuple[int, int]:
    return {
        CareHub.AMBULATORY: (0, 0),
        CareHub.OUTPATIENT: (1, 0),
        CareHub.PEDIATRIC_CARE: (0, 1),
        CareHub.NEUROLOGY: (1, 1),
        CareHub.CRITICAL_CARE: (2, 1),
        CareHub.SURGERY: (2, 2),
        CareHub.RADIOLOGY: (3, 2),
        CareHub.UNKNOWN: (4, 4),
    }[hub]
