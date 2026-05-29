from examples.mixed_job_shop import JobShop
from solverforge import Solver


def test_dynamic_mixed_scalar_list_model_solves_from_python() -> None:
    job_shop = Solver.solve(JobShop())
    assert [job.machine for job in job_shop.jobs] == [0, 0]
    assert sorted(job for queue in job_shop.queues for job in queue.jobs) == [0, 1]
