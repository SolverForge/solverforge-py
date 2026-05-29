from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass, field
from os import PathLike
from pathlib import Path
from typing import Any


_TERMINATION_FIELDS = {
    "seconds_spent_limit",
    "minutes_spent_limit",
    "best_score_limit",
    "step_count_limit",
    "unimproved_step_count_limit",
    "unimproved_seconds_spent_limit",
}


@dataclass
class TerminationConfig:
    seconds_spent_limit: int | None = None
    minutes_spent_limit: int | None = None
    best_score_limit: str | None = None
    step_count_limit: int | None = None
    unimproved_step_count_limit: int | None = None
    unimproved_seconds_spent_limit: int | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> TerminationConfig:
        if not data:
            return cls()
        unknown = sorted(set(data) - _TERMINATION_FIELDS)
        if unknown:
            joined = ", ".join(unknown)
            expected = ", ".join(sorted(_TERMINATION_FIELDS))
            msg = f"unknown SolverForge termination field(s): {joined}; expected one of: {expected}"
            raise ValueError(msg)
        return cls(
            seconds_spent_limit=data.get("seconds_spent_limit"),
            minutes_spent_limit=data.get("minutes_spent_limit"),
            best_score_limit=data.get("best_score_limit"),
            step_count_limit=data.get("step_count_limit"),
            unimproved_step_count_limit=data.get("unimproved_step_count_limit"),
            unimproved_seconds_spent_limit=data.get("unimproved_seconds_spent_limit"),
        )

    def to_dict(self) -> dict[str, Any]:
        data = {
            "seconds_spent_limit": self.seconds_spent_limit,
            "minutes_spent_limit": self.minutes_spent_limit,
            "best_score_limit": self.best_score_limit,
            "step_count_limit": self.step_count_limit,
            "unimproved_step_count_limit": self.unimproved_step_count_limit,
            "unimproved_seconds_spent_limit": self.unimproved_seconds_spent_limit,
        }
        return {key: value for key, value in data.items() if value is not None}


@dataclass
class SolverConfig:
    seconds_spent_limit: int | None = None
    random_seed: int | None = None
    phases: list[dict[str, Any]] = field(default_factory=list)
    termination: TerminationConfig | dict[str, Any] | None = None
    extra: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SolverConfig:
        termination_data = _termination_data_from_config_dict(data)
        termination = TerminationConfig.from_dict(termination_data)
        extra = {
            key: deepcopy(value)
            for key, value in data.items()
            if key not in {"random_seed", "termination", "phases"} | _TERMINATION_FIELDS
        }
        return cls(
            seconds_spent_limit=None,
            random_seed=data.get("random_seed"),
            phases=_normalize_phases(list(data.get("phases", []))),
            termination=termination,
            extra=extra,
        )

    @classmethod
    def from_toml(cls, text: str) -> SolverConfig:
        import tomllib

        return cls.from_dict(tomllib.loads(text))

    @classmethod
    def load(cls, path: str | PathLike[str] = "solver.toml") -> SolverConfig:
        return cls.from_toml(Path(path).read_text(encoding="utf-8"))

    @classmethod
    def from_file(cls, path: str | PathLike[str] = "solver.toml") -> SolverConfig:
        return cls.load(path)

    def to_dict(self) -> dict[str, Any]:
        data: dict[str, Any] = deepcopy(self.extra)
        if self.random_seed is not None:
            data["random_seed"] = self.random_seed
        termination = _termination_to_dict(self.termination)
        if self.seconds_spent_limit is not None:
            termination["seconds_spent_limit"] = self.seconds_spent_limit
        if termination:
            data["termination"] = termination
        if self.phases:
            data["phases"] = _normalize_phases(self.phases)
        return data


def _resolve_config(config: SolverConfig | dict[str, Any] | None) -> dict[str, Any] | None:
    if isinstance(config, SolverConfig):
        return config.to_dict()
    if config is not None:
        return SolverConfig.from_dict(config).to_dict()
    path = Path("solver.toml")
    if path.is_file():
        return SolverConfig.load(path).to_dict()
    return None


def _termination_to_dict(
    termination: TerminationConfig | dict[str, Any] | None,
) -> dict[str, Any]:
    if termination is None:
        return {}
    if isinstance(termination, TerminationConfig):
        return termination.to_dict()
    return TerminationConfig.from_dict(dict(termination)).to_dict()


def _termination_data_from_config_dict(data: dict[str, Any]) -> dict[str, Any]:
    termination = dict(data.get("termination") or {})
    for key in _TERMINATION_FIELDS:
        if key not in data:
            continue
        if key in termination and termination[key] != data[key]:
            msg = (
                f"conflicting SolverForge termination field `{key}`: "
                "set it either at the top level or under `termination`, not both"
            )
            raise ValueError(msg)
        termination[key] = deepcopy(data[key])
    return termination


def _normalize_phases(phases: list[dict[str, Any]]) -> list[dict[str, Any]]:
    normalized: list[dict[str, Any]] = []
    for phase in phases:
        phase_copy = deepcopy(phase)
        if "termination" in phase_copy and phase_copy["termination"] is not None:
            phase_copy["termination"] = TerminationConfig.from_dict(
                dict(phase_copy["termination"])
            ).to_dict()
        if "child_phases" in phase_copy:
            phase_copy["child_phases"] = _normalize_phases(list(phase_copy["child_phases"]))
        normalized.append(phase_copy)
    return normalized
