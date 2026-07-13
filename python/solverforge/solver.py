from __future__ import annotations

from typing import Any

from . import _native
from .config import SolverConfig, _resolve_config
from .model import _compiled_schema_for_solution


class Solver:
    @staticmethod
    def solve(
        solution: object, config: SolverConfig | dict[str, Any] | None = None
    ) -> object:
        schema = _compiled_schema_for_solution(solution)
        return _native.solve(solution, schema, _resolve_config(config))

    @staticmethod
    def analyze(solution: object) -> object:
        schema = _compiled_schema_for_solution(solution)
        return _native.calculate_score(solution, schema)
