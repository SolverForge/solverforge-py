# SolverForge Deliveries Python Example

Python port of the `uc-deliveries` example. It models deliveries as problem
facts, vehicles as route-owning planning entities, and uses SolverForge Python
planning-list variables with CVRP route hooks and shadow-variable refresh
callbacks.

It demonstrates:

- `Delivery` problem facts with demand, coordinates, service duration, and time
  windows.
- `Vehicle` planning entities with `Vehicle.delivery_order` as a planning list
  variable over `delivery_indices`.
- route callbacks for depot lookup, metric class, travel time, and feasibility.
- route shadow metrics for total demand, capacity overage, travel time, and time
  window violations.
- stock `ConstraintFactory.for_each_unassigned_element(...)` scoring for
  unassigned deliveries.
- seeded `PHILADELPHIA`, `HARTFORD`, and `FIRENZE` data sets.
- list cheapest-insertion construction, list k-opt construction polish, and
  late-acceptance list change/swap/reverse local search.
- a FastAPI app with retained `SolverManager` jobs, live SSE, exact snapshots,
  route snapshots, analysis, pause, resume, cancel, terminal-job delete, and
  delivery-insertion recommendations.
- shared SolverForge UI assets served from the native `solverforge-ui` bridge,
  with only app-specific browser modules under this example's `static/` tree.

For this source checkout, run the example through the root Makefile:

```sh
make deliveries-run PORT=7861
```

For a terminal-only solve:

```sh
make deliveries-solve
```

After the verified `0.5.0` artifacts are approved and published to PyPI, the
same source-checkout example can be run against the installed package:

```sh
python3.14 -m venv .venv-deliveries
. .venv-deliveries/bin/activate
python -m pip install "solverforge[examples]==0.5.0"
python -m examples.solverforge_deliveries
```

Then open `http://127.0.0.1:7861`.

For a terminal-only solve against the installed package:

```sh
python -m examples.solverforge_deliveries --solve
```

Use `python -m examples.solverforge_deliveries --host 0.0.0.0 --port 7862` to
override the bind address.

## Layout

The Python example mirrors the Rust `uc-deliveries/src` ownership tree:

- `src/domain`: `Delivery`, `Vehicle`, `DeliveryPlan`, route metrics, route
  snapshots, and insertion ranking.
- `src/constraints`: declarative stock SolverForge constraints.
- `src/data/data_seed`: seeded city fixtures and initial route assignment.
- `src/solver/service`: retained-job orchestration and event payloads.
- `src/api`: DTO conversion, FastAPI routes, and SSE streaming.

## API Surface

- `GET /health`, `/info`, `/demo-data`, `/demo-data/{demo_id}`
- `POST /jobs`
- `GET /jobs/{id}`, `/jobs/{id}/status`, `/jobs/{id}/snapshot`,
  `/jobs/{id}/analysis`, `/jobs/{id}/routes`, `/jobs/{id}/events`
- `POST /jobs/{id}/pause`, `/jobs/{id}/resume`, `/jobs/{id}/cancel`
- `DELETE /jobs/{id}` after a job reaches a terminal state
- `POST /recommendations/delivery-insertions`

Run `make test-deliveries` to validate the model, tree shape, static app,
shared UI asset route, retained snapshots, route snapshots, analysis payloads,
insertion recommendations, async controls, and Playwright browser recommendation
workflow.
