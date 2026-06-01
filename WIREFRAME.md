# SolverForge Python Wireframe

This is the as-built map for the current `solverforge-py` checkout. It describes
the public Python binding, native Rust bridge, retained lifecycle, PyPI artifact
shape, and hospital example UI/API surface.

## Repository Surface

- `python/solverforge/`: public Python package, decorators, score classes,
  stream API, config normalization, synchronous solver, retained manager, and
  native type stubs.
- `src/`: PyO3 extension and Rust-owned dynamic state, schema parsing,
  callback invocation, dynamic scalar/list runtime slots, constraints, solver
  handoff, and retained manager binding.
- `tests/python/`: public API, config, scalar/list/mixed solving, callback
  traceback, manager, threading, and hospital app regressions.
- `tests/rust/`: native score, state clone, runtime slot, descriptor, constraint
  set, and manager coverage.
- `examples/`: runnable Python planning examples.
- `examples/solverforge_hospital/`: FastAPI hospital demo with static UI,
  generated UI model, canonical `LARGE` data, solver config, and retained job
  lifecycle.
- `docs/`: upstream bridge, callback, threading, non-goal, and dynamic move
  parity contracts.

## Python Package API

The package exports `Solver`, `SolverManager`, `SolverConfig`,
`TerminationConfig`, `ConstraintFactory`, score classes, stable package error
classes, `joiner`, `console`, and decorators/fields for model authoring:

- `@planning_solution`, `@planning_entity`, `@problem_fact`
- `planning_id`, `planning_variable`, `planning_list_variable`
- `@constraint_provider`, `@scalar_group`, `@conflict_repair`

Solutions are normal Python objects. Entity and fact collections are inferred
from type hints where available, then from instance lists. The native module
mutates Rust-owned dynamic state and exports the solved state back to Python.

## Constraint Surface

Supported stream shapes:

- unary `for_each(...).filter(...).penalize/reward(...).named(...)`
- binary `for_each(...).join(...).filter(...).penalize/reward(...).named(...)`
- grouped count `for_each(...).group_by(...).filter(...).penalize/reward(...).named(...)`
- balance `for_each(...).balance(...).filter(...).penalize/reward(...).named(...)`

Weights may be score objects, integer/sequence values, or Python callbacks.
`joiner.equal(...)` and `joiner.equal_bi(...)` use Python equality rather than
string representations. Top-level `ConstraintFactory.join`, `group_by`,
`if_exists`, `if_not_exists`, and `flattened` remain explicit unsupported
methods. Stream-level `for_each(...).join(...)` and
`for_each(...).group_by(...)` are the supported join and grouped-count surfaces.

## Solver And Runtime Flow

`Solver.solve(solution, config=None)` builds a schema, imports Python data into
Rust dynamic state, builds the upstream dynamic runtime model, runs SolverForge,
and writes assignments plus score back onto the original solution.

`Solver.analyze(solution)` evaluates callback constraints against the imported
state and writes the calculated score to `solution.score`.

`SolverManager(config=None)` wraps upstream retained jobs. It exposes
`solve`, `get_status`, `events`, `wait`, `snapshot`, `pause`, `resume`,
`cancel`, and `delete`. Snapshots are deep-copied Python solutions exported from
Rust-owned retained state.

Config may be a `SolverConfig`, a dict, or `None`. When `None`, `solver.toml` in
the current directory is loaded if present. Termination fields are accepted at
the top level or under `termination`, and phase termination is normalized before
handoff.

## Dynamic Move Support

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
- `list_reverse_move_selector`, `k_opt_move_selector`
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

The hospital app is source-checkout example material. It is included in the
source distribution for reproducible builds and development, but it is not
installed into the runtime wheel.

## PyPI Artifact Shape

The installable wheel contains only:

- `solverforge/*.py`
- `solverforge/_native.*`
- `solverforge/_native.pyi`
- `solverforge/py.typed`
- `solverforge-*.dist-info/`

The source distribution also carries the repository tests, examples, and docs.
SolverForge Rust dependencies are declared from the exact git revision in
`Cargo.toml` and locked in `Cargo.lock`; release automation verifies that
manifest/lockfile source of truth instead of inspecting a mutable sibling
checkout.

## Makefile And Validation Flow

The root Makefile is the maintainer entry point:

- `make develop`: release local extension install
- `make test`: Rust plus Python tests
- `make lint`: rustfmt check, ruff, mypy, clippy
- `make ci-local`: local CI simulation
- `make build-dist`: release source distribution plus local wheel
- `make dist-check`: metadata and artifact-content checks
- `make pre-release`: CI simulation plus release artifact checks
- `make hospital-run` / `make hospital-solve`: browser or terminal hospital demo

Use `make docs-check` after documentation edits so `README.md`, `AGENTS.md`,
and this wireframe stay present and current.
