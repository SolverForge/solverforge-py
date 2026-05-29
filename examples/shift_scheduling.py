from solverforge import (
    ConstraintFactory,
    HardSoftScore,
    Solver,
    console,
    constraint_provider,
    planning_entity,
    planning_solution,
    planning_variable,
)


@planning_entity
class Shift:
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self, required: bool = True, nurse: int | None = None) -> None:
        self.required = required
        self.nurse = nurse


@constraint_provider
def constraints(factory: ConstraintFactory):
    return [
        factory.for_each(Shift)
        .filter(lambda shift: shift.required and shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("required shift is unassigned")
    ]


@planning_solution(score=HardSoftScore, constraints=constraints)
class Schedule:
    shifts: list[Shift]

    def __init__(self, shifts: list[Shift], nurses: list[int]) -> None:
        self.shifts = shifts
        self.nurses = nurses
        self.score = None


if __name__ == "__main__":
    console.init()
    solved = Solver.solve(Schedule([Shift(), Shift()], [0, 1]))
    print(solved.score)
