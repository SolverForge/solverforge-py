from solverforge import planning_entity, planning_solution, planning_variable, problem_fact
from solverforge.model import build_schema


@planning_entity
class Task:
    worker = planning_variable(value_range_provider="workers", allows_unassigned=True)

    def __init__(self) -> None:
        self.worker = None


@planning_solution()
class Plan:
    tasks: list[Task]

    def __init__(self) -> None:
        self.tasks = [Task()]
        self.workers = [0, 1]
        self.score = None


def test_schema_collects_entity_and_variable_metadata() -> None:
    schema = build_schema(Plan())
    assert schema["solution_type"] == "Plan"
    assert schema["entities"][0]["type_name"] == "Task"
    assert schema["entities"][0]["fields"][0]["name"] == "worker"


@problem_fact
class CalendarDay:
    def __init__(self, day: str) -> None:
        self.day = day


@planning_solution()
class EmptyAnnotatedPlan:
    tasks: "list[Task]"
    days: "list[CalendarDay]"

    def __init__(self) -> None:
        self.tasks = []
        self.days = []
        self.score = None


def test_schema_resolves_deferred_annotations_for_empty_collections() -> None:
    schema = build_schema(EmptyAnnotatedPlan())

    assert schema["entities"][0]["type_name"] == "Task"
    assert schema["entities"][0]["collection"] == "tasks"
    assert schema["facts"][0]["type_name"] == "CalendarDay"
    assert schema["facts"][0]["collection"] == "days"
