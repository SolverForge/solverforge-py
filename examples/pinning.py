"""Entity pinning: a locked route survives the solve.

Run `python examples/pinning.py` from the repository root. Vehicle 1 arrives with
its visits already planned and is pinned, so construction and local search leave
its route exactly as it is; vehicle 0 is free and receives every remaining visit.
"""

from solverforge import (
    ConstraintFactory,
    SoftScore,
    Solver,
    console,
    constraint_provider,
    planning_entity,
    planning_list_variable,
    planning_pin,
    planning_solution,
)


@planning_entity
class Vehicle:
    pinned = planning_pin()
    visits = planning_list_variable(element_collection="visit_values")

    def __init__(
        self, name: str, pinned: bool, visits: list[int] | None = None
    ) -> None:
        self.name = name
        self.pinned = pinned
        self.visits: list[int] = list(visits or [])


@constraint_provider
def constraints(factory: ConstraintFactory):
    # One soft count per owner carrying more than one visit, so the model prefers
    # a single carrier. The pinned vehicle cannot release its two visits, which is
    # what forces the free one to take the rest.
    return [
        factory.for_each(Vehicle)
        .filter(lambda vehicle: len(vehicle.visits) > 1)
        .penalize(SoftScore.of(1))
        .named("long route"),
    ]


@planning_solution(score=SoftScore, constraints=constraints)
class Routes:
    vehicles: list[Vehicle]

    def __init__(self, vehicles: list[Vehicle]) -> None:
        self.vehicles = vehicles
        self.visit_values = [0, 1, 2, 3, 4]
        self.score = None


if __name__ == "__main__":
    console.init()
    plan = Routes(
        [
            Vehicle("free", pinned=False),
            Vehicle("pinned", pinned=True, visits=[3, 4]),
        ]
    )
    print("before:", {vehicle.name: vehicle.visits for vehicle in plan.vehicles})

    solved = Solver.solve(plan, {"termination": {"step_count_limit": 300}})

    print("after: ", {vehicle.name: vehicle.visits for vehicle in solved.vehicles})
    print("score: ", solved.score)
