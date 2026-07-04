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
  Versioned `/sf/*` asset names are served only when the pinned `solverforge-ui`
  crate owns that exact file; stale versioned asset requests fail instead of
  aliasing to current bytes.
- Documentation is intentionally kept in `README.md`, `AGENTS.md`, this
  wireframe, and the example READMEs. There is no separate `docs/` directory.

## Python Package API

The package exports `Solver`, `SolverManager`, `JobHandle`, `SolverConfig`,
`TerminationConfig`, `ConstraintFactory`, score classes, stable package error
classes (`SolverForgeError`, `CallbackError`, `ModelValidationError`, and
`NativeBridgeError`), `joiner`, `console`, `ui`, and decorators/helpers for
model authoring:

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
List variables may declare element-owner callbacks, solution-level route
callbacks, entity-scoped route callbacks, and field-backed route metadata for
owner-aware list moves, list precedence/makespan scoring, and CVRP-style
construction. Field-backed route metadata can supply depot, metric-class,
distance-matrix, capacity, and demand data without per-query Python callbacks.
Shadow update listeners refresh native-owned derived fields and those fields are
exported back to Python objects after solve, analyze, and retained snapshot
export.

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

`SolverManager(config=None)` wraps upstream retained jobs. It exposes `solve`,
which returns `JobHandle(job_id=...)`, plus `get_status`, `events`, `wait`,
`snapshot`, `pause`, `resume`, `cancel`, and `delete`. Submitted Python
solutions are deep-copied before import so retained jobs do not mutate the
caller's object. Snapshots are deep-copied Python solutions exported from
Rust-owned retained state. Example app bootstrap and retained event payloads
report the current score from the attached solution snapshot when one is
available, and use that score as the best-score fallback only when the retained
manager event has not supplied a best score.

Config may be a `SolverConfig`, a dict, or `None`. When `None`, `solver.toml` in
the current directory is loaded if present. Termination fields are accepted at
the top level or under `termination`, and phase termination is normalized before
handoff.

## Upstream Boundary

The binding works against public upstream crates only. `PlanningSolution`
requires `Clone + Send + Sync + 'static`. `ConstraintSet<S, Score>` is the
public scoring seam used by the dynamic adapter. Logical descriptor IDs and
dynamic backend/slot contracts are exposed through `solverforge-core`, with
`solverforge-bridge` re-exporting them for binding crates, so dynamic Python
entity classes do not need distinct Rust row types for descriptor identity.

Upstream exposes `run_solver_with_config_parts`, allowing the binding to pass
already-built descriptors, constraint sets, runtime model pieces, config, and
runtime values instead of macro-style `fn() -> T` factories.
`SolverRuntime::detached()` is used for synchronous binding solves that should
use the upstream runner without copying retained manager internals. Retained
Python jobs use upstream `SolverManager<PyDynamicSolution>` and the same
`Solvable`/runtime path as synchronous dynamic solves.

Rust-only custom search and partitioner registration remain Rust-only unless
upstream exposes a public host-language binding seam.

## Dynamic Move Support

Dynamic Python models support the public upstream move selectors listed below
without exposing Rust macros, generating Rust, or silently degrading one
selector into another. `DynamicScalar` and `DynamicList` are first-class dynamic
move families; compound scalar hooks use Python callbacks because upstream
grouped/conflict registration is typed Rust.

Dynamic construction phases:

- scalar `first_fit` and `cheapest_insertion`
- scalar assignment-group `first_fit` and `cheapest_insertion` when a
  construction phase declares `group_name`
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

The dynamic runtime uses one move family with scalar, list, and cartesian
variants. `DynamicScalar` covers change, swap, pillar change/swap,
ruin-recreate, grouped, conflict repair, and compound conflict repair moves.
`DynamicList` covers change, swap, multi-swap, permute, sublist change/swap,
reverse, k-opt, and ruin moves. Nearby selectors are selector strategies, not
separate move mutations; they emit the corresponding change or swap variants
after pruning and ordering.

Grouped scalar and conflict repair selectors use Python-declared callbacks that
return compound scalar edit candidates. Rust owns the actual solver state and
applies those edits through `DynamicScalarVariableSlot<PyDynamicSolution>`, so
clone, snapshot, and move mutation semantics remain Rust-side.

Implementation status is complete across the relevant surfaces:
`python/solverforge/decorators.py` exposes `@scalar_group`,
`@conflict_repair`, and `@planning_solution(..., scalar_groups=...,
conflict_repairs=...)`; `python/solverforge/model.py` emits and validates those
schema registries; `python/solverforge/__init__.py` exports the public helpers;
the Rust schema carries scalar group and conflict repair callback registries;
`src/runtime/dynamic_scalar_search.rs` implements scalar, list, and cartesian
dynamic moves; Python tests cover scalar assignment/change, limited
neighborhood, swap with tabu, pillar moves, ruin-recreate, cartesian
composition, grouped scalar, conflict repair, compound conflict repair, list
selectors, mixed scalar/list union, list/list and scalar/list cartesian
composition, and assigned-element preservation. Rust tests cover score, clone
independence, descriptors, runtime slots, constraint set behavior, and manager
type wiring. `DynamicListChange` must not exist as a top-level
`solverforge-py` runtime type.

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
SSE, shows score/telemetry state from the latest retained event or snapshot,
opens snapshot analysis, and supports pause, resume, cancel, and terminal delete
controls.

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
`examples/solverforge_deliveries/static`. Browser smoke tests stub only known
OpenStreetMap tile image requests so route markers and insertion workflows stay
hermetic while real app console/page errors still fail the test.

## PyPI Artifact Shape

The installable wheel contains only:

- `solverforge/*.py`
- `solverforge/_native.*`
- `solverforge/_native.pyi`
- `solverforge/py.typed`
- `solverforge-*.dist-info/`

The native extension embeds shared `solverforge-ui` assets and exposes them via
`solverforge.ui.asset()` for example applications and other Python HTTP hosts.

The source distribution also carries the repository tests and examples.
SolverForge Rust dependencies and shared UI assets are declared from exact
release versions in `Cargo.toml` and locked in `Cargo.lock`; release automation
verifies that manifest/lockfile source of truth instead of inspecting a mutable
sibling checkout.

As of July 4, 2026, this checkout is tagged as `v0.5.0` and the GitHub release
workflow has built and verified the source distribution plus Linux, macOS, and
Windows wheels. PyPI publication is still waiting at the reviewed `pypi`
environment, so public PyPI and TestPyPI indexes still report `0.4.0` as the
latest published package.

## Makefile And Validation Flow

The root Makefile is the maintainer entry point:

- `make develop`: release local extension install
- `make install-playwright-system-deps`: Chromium and Linux browser libraries
  for Playwright tests in lean CI images
- `make test`: Rust plus Python tests
- `make lint`: rustfmt check, ruff, mypy, clippy
- `make ci-local`: local CI simulation
- `make test-examples-browser`: Playwright browser tests for both example apps
- `make build-dist`: release source distribution plus local wheel
- `make dist-check`: metadata and artifact-content checks
- `make release-base-check`: exact SolverForge crates.io dependency-base check
- `make pre-release`: release-base check, CI simulation, and release artifact checks
- `make hospital-run` / `make hospital-solve`: browser or terminal hospital demo
- `make test-hospital`: hospital model and FastAPI/frontend tests
- `make deliveries-run` / `make deliveries-solve`: browser or terminal deliveries demo
- `make test-deliveries`: deliveries model and FastAPI/frontend tests

Use `make docs-check` after documentation edits so the tracked README, AGENTS,
WIREFRAME, and example README surfaces stay present and current. Dynamic move
parity closeout searches should stay clean for
`cartesian_product_move_selector is not yet bound`, unsupported dynamic selector
claims, stale `_ => false` selector fallbacks, and any top-level
`DynamicListChange` runtime type.
