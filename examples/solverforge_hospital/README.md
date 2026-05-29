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
- the canonical `LARGE` dataset: 50 employees, 688 unassigned shifts, and the
  original 30-second solver config.
- dynamic scalar cheapest-insertion construction plus the original nearby
  change/swap late-acceptance local search.
- the hospital browser shell, by-location and by-employee schedule tabs, shared
  SolverForge UI assets, and the same retained async `/jobs` lifecycle used by
  the Rust app: background
  `SolverManager` jobs, live SSE, exact snapshots, analysis, pause, resume,
  cancel, and terminal-job delete.
- the upstream SolverForge Rust console in both terminal-only and FastAPI CLI
  server modes.

Run it from the `solverforge-py` repo root:

```sh
make hospital-run
```

Then open `http://127.0.0.1:7860`.

For a terminal-only solve:

```sh
make hospital-solve
```

Use `APP_HOST=0.0.0.0 PORT=7861 make hospital-run` to override the bind address.

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

Run `make test-hospital` to validate the model, tree shape, static app, retained
snapshots, SSE stream, analysis payloads, and async controls.
