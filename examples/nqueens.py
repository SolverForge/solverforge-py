from solverforge import ConstraintFactory, HardSoftScore, Solver, console, constraint_provider
from solverforge import planning_entity, planning_solution, planning_variable


@planning_entity
class Queen:
    row = planning_variable(value_range_provider="rows", allows_unassigned=True)

    def __init__(self, column: int, row: int | None = None) -> None:
        self.column = column
        self.row = row


@constraint_provider
def constraints(factory: ConstraintFactory):
    return [
        factory.for_each(Queen)
        .filter(lambda queen: queen.row is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("unassigned queen")
    ]


@planning_solution(score=HardSoftScore, constraints=constraints)
class NQueens:
    queens: list[Queen]

    def __init__(self, n: int) -> None:
        self.rows = list(range(n))
        self.queens = [Queen(column) for column in range(n)]
        self.score = None


if __name__ == "__main__":
    console.init()
    print(Solver.solve(NQueens(4)).score)
