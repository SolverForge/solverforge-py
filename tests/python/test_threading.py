from concurrent.futures import ThreadPoolExecutor

from tests.python.test_scalar_solving import Schedule
from solverforge import Solver


def test_concurrent_python_solves_do_not_share_solution_state() -> None:
    with ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(lambda _: Solver.solve(Schedule()), range(2)))
    assert results[0] is not results[1]

