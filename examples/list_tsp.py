from solverforge import planning_entity, planning_list_variable, planning_solution


@planning_entity
class Tour:
    visits = planning_list_variable(element_collection="visit_values")

    def __init__(self, visits: list[int] | None = None) -> None:
        self.visits = visits or []


@planning_solution()
class Tsp:
    tours: list[Tour]

    def __init__(self) -> None:
        self.tours = [Tour()]
        self.visit_values = [0, 1, 2, 3]
        self.score = None

