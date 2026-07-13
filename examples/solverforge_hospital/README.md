# SolverForge Hospital Python Example

This example rewrites the core `solverforge-hospital` planning model with the
`solverforge-py` callback surface and serves the original Rust counterpart's
browser frontend from a FastAPI app.

It demonstrates:

- `Employee` problem facts.
- `Shift` planning entities.
- one scalar planning variable, `Shift.employee_idx`.
- hard constraints for assigned shifts, required skills, employee
  unavailability, overlapping shifts, one shift per day, and 10-hour rest.
- soft constraints for undesired days, desired days, and workload balance.
- the canonical `LARGE` dataset: 50 employees and 688 initially unassigned
  shifts.
- a row candidate callback that excludes employees without the required skill
  or with overlapping unavailability before construction, while row-backed
  candidate/distance metadata drives nearby search.
- the reproducible seed-1 solver config: 30-second and 5-second-unimproved
  termination, cheapest-insertion construction with
  `assign_when_candidate_exists`, and max-10 nearby change/swap late-acceptance
  local search with a first-best-score-improving forager.
- the hospital browser shell, by-location and by-employee schedule tabs, shared
  SolverForge UI assets served from the native `solverforge-ui` bridge, and the
  same retained async `/jobs` lifecycle used by the Rust app: background
  `SolverManager` jobs, live SSE, exact snapshots, analysis, pause, resume,
  cancel, and terminal-job delete.
- the upstream SolverForge Rust console in both terminal-only and FastAPI CLI
  server modes.

For this source checkout, run the example through the root Makefile:

```sh
make hospital-run
```

For a terminal-only solve:

```sh
make hospital-solve
```

The same source-checkout example can be run against the published `0.6.0`
package:

```sh
python3.14 -m venv .venv-hospital
. .venv-hospital/bin/activate
python -m pip install "solverforge[examples]==0.6.0"
python -m examples.solverforge_hospital
```

Then open `http://127.0.0.1:7860`.

For a terminal-only solve against the installed package:

```sh
python -m examples.solverforge_hospital --solve
```

Use `python -m examples.solverforge_hospital --host 0.0.0.0 --port 7861` to
override the bind address.

## Layout

The Python example mirrors the Rust `uc-hospital/src` ownership tree:

- `src/domain`: `CareHub`, `Employee`, `Shift`, `HospitalPlan`, and domain math.
- `src/constraints`: one module per SolverForge constraint.
- `src/data/data_seed`: canonical `LARGE` fixture loading and seed projections.
- `src/solver/service`: retained-job orchestration and event payloads.
- `src/api`: DTO conversion, FastAPI routes, and SSE streaming.

## API Surface

- `GET /health`, `/info`, `/demo-data`, `/demo-data/LARGE`, `/solve-summary`
- `POST /jobs`
- `GET /jobs/{id}`, `/jobs/{id}/status`, `/jobs/{id}/snapshot`,
  `/jobs/{id}/analysis`, `/jobs/{id}/events`
- `POST /jobs/{id}/pause`, `/jobs/{id}/resume`, `/jobs/{id}/cancel`
- `DELETE /jobs/{id}` after a job reaches a terminal state

Run `make test-hospital` to validate the model, tree shape, static app, shared
UI asset route, retained snapshots, SSE stream, analysis payloads, async
controls, and Playwright browser solve/analysis workflow.
