from solverforge import planning_entity, planning_list_variable, planning_solution


@planning_entity
class Vehicle:
    visits = planning_list_variable(element_collection="visit_values")

    def __init__(self, vehicle_id: int) -> None:
        self.vehicle_id = vehicle_id
        self.visits: list[int] = []


@planning_solution()
class Vrp:
    vehicles: list[Vehicle]

    def __init__(self) -> None:
        self.vehicles = [Vehicle(0), Vehicle(1)]
        self.visit_values = [0, 1, 2, 3]
        self.score = None

