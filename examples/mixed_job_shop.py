from solverforge import planning_entity, planning_list_variable, planning_solution, planning_variable


@planning_entity
class Job:
    machine = planning_variable(value_range_provider="machines")

    def __init__(self, machine: int | None = None) -> None:
        self.machine = machine


@planning_entity
class MachineQueue:
    jobs = planning_list_variable(element_collection="job_values")

    def __init__(self) -> None:
        self.jobs: list[int] = []


@planning_solution()
class JobShop:
    jobs: list[Job]
    queues: list[MachineQueue]

    def __init__(self) -> None:
        self.jobs = [Job(), Job()]
        self.queues = [MachineQueue(), MachineQueue()]
        self.machines = [0, 1]
        self.job_values = [0, 1]
        self.score = None
