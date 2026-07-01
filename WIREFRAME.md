# SolverForge Python Wireframe

This is the as-built map for the current `solverforge-py` checkout. It describes
the public Python binding, native Rust bridge, retained lifecycle, PyPI artifact
shape, and source-checkout example UI/API surfaces.

## Repository Surface

- `python/solverforge/`: public Python package, decorators, score classes,
  stream API, config normalization, synchronous solver, retained manager, and
  native type stubs.
- `src/`: PyO3 extension and Rust-owned dynamic state, schema parsing,
  callback invocation, dynamic scalar/list runtime slots, constraints, solver
  handoff, and retained manager binding.
- `tests/python/`: public API, config, scalar/list/mixed solving, callback
  traceback, manager, threading, shared UI asset, and example app regressions.
- `tests/rust/`: native score, state clone, runtime slot, descriptor, constraint
  set, and manager coverage.
- `examples/`: runnable Python planning examples.
- `examples/solverforge_hospital/`: FastAPI hospital demo with static UI,
  generated UI model, canonical `LARGE` data, solver config, and retained job
  lifecycle.
- `examples/solverforge_deliveries/`: FastAPI deliveries demo with static UI,
  generated UI model, seeded city data, CVRP-style route hooks, route snapshots,
  retained job lifecycle, and insertion recommendations.
- Shared `/sf/*` frontend assets are embedded by the `solverforge-ui` Rust crate
  and exposed to Python through `solverforge.ui`, not copied into each example.
- `docs/`: callback, upstream bridge, threading, release, goal/non-goal, and
  dynamic move parity contracts.

## Python Package API

The package exports `Solver`, `SolverManager`, `SolverConfig`,
`TerminationConfig`, `ConstraintFactory`, score classes, stable package error
classes, `joiner`, `console`, `ui`, and decorators/helpers for model authoring:

- `@planning_solution`, `@planning_entity`, `@problem_fact`
- `planning_id`, `planning_variable`, `planning_list_variable`
- `@constraint_provider`, `@scalar_group`, `@conflict_repair`
- `scalar_assignment_group(...)`, `ScalarAssignmentGroup`, `ScalarGroupLimits`
- `shadow_variable_updates(...)`
- `indexed_presence(...)`

Solutions are normal Python objects. Entity and fact collections are inferred
from type hints where available, then from instance lists. The native module
mutates Rust-owned dynamic state and exports the solved state back to Python.
Python callback solution views preserve ordinary solution-level lookup context
while projecting entity/fact collections from Rust-owned state, so preview clones
do not share mutable row objects with the working solution.
List variables may declare element-owner and route callbacks for owner-aware
list moves, list precedence/makespan scoring, and CVRP-style construction.
Field-backed route metadata can supply depot, metric-class, distance-matrix,
capacity, and demand data without per-query Python callbacks. Shadow update
listeners refresh native-owned derived fields and those fields are exported back
to Python objects after solve, analyze, and retained snapshot export.

## Constraint Surface

Supported stream shapes:

- unary `for_each(...).filter(...).penalize/reward(...).named(...)`
- binary `for_each(...).join(...).filter(...).penalize/reward(...).named(...)`
- grouped count `for_each(...).group_by(...).filter(...).penalize/reward(...).named(...)`
- balance `for_each(...).balance(...).filter(...).penalize/reward(...).named(...)`
- list-unassigned element
  `for_each_unassigned_element(owner_entity_type, variable_name).filter(...).penalize/reward(...).named(...)`
- list precedence/makespan
  `list_precedence_makespan(owner_entity_type, variable_name).named(...)`

Ordinary penalize/reward stream weights may be score objects, integer/sequence
values, or Python callbacks. List precedence/makespan scoring is computed
natively from owner, duration, and successor callbacks on the planning list
variable.
Grouped streams support the default count collector and `indexed_presence(...)`
for run/range presence analysis.
`joiner.equal(...)` and `joiner.equal_bi(...)` use Python equality rather than
string representations. Top-level `ConstraintFactory.join`, `group_by`,
`if_exists`, `if_not_exists`, and `flattened` remain explicit unsupported
methods. Stream-level `for_each(...).join(...)` and
`for_each(...).group_by(...)` are the supported join and grouped-count surfaces.

## Solver And Runtime Flow

`Solver.solve(solution, config=None)` builds a schema, imports Python data into
Rust dynamic state, builds the upstream dynamic runtime model, runs SolverForge,
and writes assignments plus score back onto the original solution.

`Solver.analyze(solution)` imports Python data, refreshes native-owned shadow
fields, evaluates callback constraints against the imported state, exports
refreshed assignments/shadow fields back to the Python object, and writes the
calculated score to `solution.score`.

`SolverManager(config=None)` wraps upstream retained jobs. It exposes
`solve`, `get_status`, `events`, `wait`, `snapshot`, `pause`, `resume`,
`cancel`, and `delete`. Submitted Python solutions are deep-copied before import
so retained jobs do not mutate the caller's object. Snapshots are deep-copied
Python solutions exported from Rust-owned retained state.

Config may be a `SolverConfig`, a dict, or `None`. When `None`, `solver.toml` in
the current directory is loaded if present. Termination fields are accepted at
the top level or under `termination`, and phase termination is normalized before
handoff.

## Dynamic Move Support

Dynamic construction phases:

- scalar `first_fit` and `cheapest_insertion`
- scalar assignment-group `cheapest_insertion` when a construction phase declares
  `group_name`
- list `list_cheapest_insertion` and `list_regret_insertion`
- route-aware list `list_clarke_wright` and `list_k_opt`

Dynamic scalar selectors:

- `change_move_selector`, `swap_move_selector`
- `nearby_change_move_selector`, `nearby_swap_move_selector`
- `pillar_change_move_selector`, `pillar_swap_move_selector`
- `ruin_recreate_move_selector`
- `grouped_scalar_move_selector`
- `conflict_repair_move_selector`
- `compound_conflict_repair_move_selector`

Dynamic list selectors:

- `list_change_move_selector`, `nearby_list_change_move_selector`
- `list_swap_move_selector`, `nearby_list_swap_move_selector`
- `sublist_change_move_selector`, `sublist_swap_move_selector`
- `list_reverse_move_selector`, `list_permute_move_selector`,
  `list_precedence_move_selector`, `k_opt_move_selector`
- `list_ruin_move_selector`

Selector combinators `limited_neighborhood`, `union_move_selector`, and
two-child `cartesian_product_move_selector` compose supported scalar and list
selectors. Rust macro models remain the fastest path; Python bindings preserve
the Rust engine while paying dynamic callback overhead.

## Hospital Example UI/API

`examples/solverforge_hospital` mirrors the Rust `uc-hospital/src` ownership
tree with `domain/`, `constraints/`, `data/data_seed/`, `solver/service/`, and
`api/`. It exposes:

- `GET /health`, `/info`, `/demo-data`, `/demo-data/LARGE`, `/solve-summary`
- `POST /jobs`
- `GET /jobs/{id}`, `/jobs/{id}/status`, `/jobs/{id}/snapshot`,
  `/jobs/{id}/analysis`, `/jobs/{id}/events`
- `POST /jobs/{id}/pause`, `/resume`, `/cancel`
- `DELETE /jobs/{id}`

The static app loads `sf-config.json` and `generated/ui-model.json`, renders
schedule views by location and by employee, streams retained job events over
SSE, shows score/telemetry state, opens snapshot analysis, and supports pause,
resume, cancel, and terminal delete controls.

The app serves `/sf/{path}` from the native `solverforge-ui` bridge and serves
only app-specific files from `examples/solverforge_hospital/static`.

The hospital app is source-checkout example material. It is included in the
source distribution for reproducible builds and development, but it is not
installed into the runtime wheel.

## Deliveries Example UI/API

`examples/solverforge_deliveries` mirrors the Rust `uc-deliveries/src`
ownership tree with `domain/`, `constraints/`, `data/data_seed/`,
`solver/service/`, and `api/`. It models deliveries as facts and vehicles as
route-owning planning entities with `Vehicle.delivery_order` as a planning list
variable. The list variable supplies route depot, route distance, route
feasibility, and route metric-class callbacks; route metric shadows are
refreshed through `shadow_variable_updates(...)`.

It exposes:

- `GET /health`, `/info`, `/demo-data`, `/demo-data/{demo_id}`
- `POST /jobs`
- `GET /jobs/{id}`, `/jobs/{id}/status`, `/jobs/{id}/snapshot`,
  `/jobs/{id}/analysis`, `/jobs/{id}/routes`, `/jobs/{id}/events`
- `POST /jobs/{id}/pause`, `/resume`, `/cancel`
- `DELETE /jobs/{id}`
- `POST /recommendations/delivery-insertions`

The static app serves the same native `/sf/{path}` shared assets as the hospital
app and keeps deliveries-specific modules under
`examples/solverforge_deliveries/static`.

## PyPI Artifact Shape

The installable wheel contains only:

- `solverforge/*.py`
- `solverforge/_native.*`
- `solverforge/_native.pyi`
- `solverforge/py.typed`
- `solverforge-*.dist-info/`

The native extension embeds shared `solverforge-ui` assets and exposes them via
`solverforge.ui.asset()` for example applications and other Python HTTP hosts.

The source distribution also carries the repository tests, examples, docs, and
the vendored `solverforge-ui` crate needed to rebuild the embedded shared UI
assets. SolverForge Rust dependencies are declared from the exact release
version in `Cargo.toml` and locked in `Cargo.lock`; release automation verifies
that manifest/lockfile source of truth instead of inspecting a mutable sibling
checkout.

## Makefile And Validation Flow

The root Makefile is the maintainer entry point:

- `make develop`: release local extension install
- `make test`: Rust plus Python tests
- `make lint`: rustfmt check, ruff, mypy, clippy
- `make ci-local`: local CI simulation
- `make test-examples-browser`: Playwright browser tests for both example apps
- `make build-dist`: release source distribution plus local wheel
- `make dist-check`: metadata and artifact-content checks
- `make pre-release`: CI simulation plus release artifact checks
- `make hospital-run` / `make hospital-solve`: browser or terminal hospital demo
- `make test-hospital`: hospital model and FastAPI/frontend tests
- `make deliveries-run` / `make deliveries-solve`: browser or terminal deliveries demo
- `make test-deliveries`: deliveries model and FastAPI/frontend tests

Use `make docs-check` after documentation edits so the tracked README, AGENTS,
WIREFRAME, docs, example README, and vendored UI README surfaces stay present
and current.
