from __future__ import annotations

from pathlib import Path
from typing import cast

from solverforge import Solver, SolverConfig

from .constraints import delivery_constraints
from .data import demo_plan
from .domain import DeliveryPlan

ROOT_DIR = Path(__file__).resolve().parents[1]
DELIVERIES_SOLVER_CONFIG = SolverConfig.load(ROOT_DIR / "solver.toml")


def solve_demo() -> DeliveryPlan:
    return cast(DeliveryPlan, Solver.solve(demo_plan(), DELIVERIES_SOLVER_CONFIG))


def route_summary(plan: DeliveryPlan) -> list[tuple[str, int, int]]:
    return [
        (vehicle.name, len(vehicle.delivery_order), int(vehicle.route_total_demand))
        for vehicle in plan.vehicles
    ]


__all__ = [
    "DELIVERIES_SOLVER_CONFIG",
    "DeliveryPlan",
    "delivery_constraints",
    "demo_plan",
    "route_summary",
    "solve_demo",
]
