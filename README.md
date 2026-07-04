# SolverForge Python

`solverforge` is the dynamic Python binding package for SolverForge. Python
users define models with Python classes, decorators, functions, and lambdas.
They do not write Rust and they do not pass JSON to a fixed backend.

The native extension owns the working solution state in Rust so SolverForge can
clone, mutate, and snapshot solutions safely. Python callbacks are the single
constraint authoring surface.

The package targets CPython 3.14 and Rust 1.95.0. This checkout pins upstream
SolverForge Rust crates to `0.17.2` and `solverforge-ui` to `0.7.0` through
`Cargo.toml` and `Cargo.lock`. The `solverforge` `0.5.x` line is the current
dynamic binding architecture and intentionally supersedes the older incompatible
`0.2.x` and `0.3.0` artifacts in the same PyPI namespace. Those older artifacts
exposed `SolverFactory`, `PlanningVariable`, Java service requirements, and
other APIs that are not part of this package.

## Installation

This checkout builds `solverforge` `0.5.0`. As of July 4, 2026, the `v0.5.0`
tagged release artifacts have passed GitHub release verification and are waiting
at the reviewed PyPI publish environment; PyPI and TestPyPI still resolve
`solverforge` to `0.4.0`. Use the root Makefile targets for source-current
validation until that publish approval completes.

After the release approval completes:

```sh
python3.14 -m pip install "solverforge==0.5.0"
```

The installable wheel contains the core `solverforge` package, native extension,
and embedded shared `solverforge-ui` assets exposed through `solverforge.ui`.
Source-checkout examples, including the hospital and deliveries FastAPI apps and
their app-specific static files, are maintained in this repository rather than
installed into the runtime wheel.

## Examples

Run source-current examples through the root Makefile while the `0.5.0` PyPI
publish job is awaiting approval:

```sh
make hospital-run
make deliveries-run PORT=7861
```

After `0.5.0` is published to PyPI, the same source-checkout examples can be run
against the installed package without `PYTHONPATH` or an editable checkout:

```sh
python3.14 -m venv .venv-examples
. .venv-examples/bin/activate
python -m pip install "solverforge[examples]==0.5.0"
python examples/nqueens.py
python -m examples.solverforge_hospital
python -m examples.solverforge_deliveries
```

Then open `http://127.0.0.1:7860` for the hospital app or
`http://127.0.0.1:7861` for the deliveries app.

For local package development, use the root `Makefile` targets instead.

## Minimal Model

A minimal model uses Python decorators for domain metadata and plain Python
callbacks for constraints:

```python
from solverforge import (
    ConstraintFactory,
    HardSoftScore,
    Solver,
    constraint_provider,
    planning_entity,
    planning_solution,
    planning_variable,
)

@planning_entity
class Shift:
    nurse = planning_variable(value_range_provider="nurses", allows_unassigned=True)

    def __init__(self, required: bool = True, nurse: int | None = None) -> None:
        self.required = required
        self.nurse = nurse

@constraint_provider
def constraints(factory: ConstraintFactory):
    return [
        factory.for_each(Shift)
        .filter(lambda shift: shift.required and shift.nurse is None)
        .penalize(HardSoftScore.ONE_HARD)
        .named("required shift is unassigned")
    ]

@planning_solution(score=HardSoftScore, constraints=constraints)
class Schedule:
    shifts: list[Shift]

    def __init__(self, shifts: list[Shift], nurses: list[int]) -> None:
        self.shifts = shifts
        self.nurses = nurses
        self.score = None

solution = Solver.solve(Schedule([Shift(), Shift()], [0, 1]))
```

## Development

Local development is driven by the root `Makefile`, which creates `.venv`,
installs maturin and developer tools, and builds the PyO3 extension against the
current checkout.

```sh
make develop          # release native extension installed into .venv
make install-playwright-system-deps  # Linux Chromium browser libraries for CI
make test             # Rust tests with Python link setup, then pytest
make lint             # rustfmt check, ruff, mypy, and clippy
make ci-local         # local CI simulation
make pre-release      # ci-local plus release sdist/wheel checks
```

Run `make help` for focused targets such as `make test-hospital`,
`make test-deliveries`, `make test-examples-browser`, `make docs-check`,
`make release-base-check`, `make test-one TEST=pattern`, `make hospital-run`,
`make hospital-solve`, `make deliveries-run`, and `make deliveries-solve`.

## Boundaries

- No generated Rust.
- No expression-object DSL.
- No string-parsed constraints.
- No duplicate 100% Python solver.
- No private upstream SolverForge modules; bindings use the public dynamic bridge
  contract.
- No fixed pre-modeled JSON-only backends.
- No upstream SolverForge mutation in this repository; upstream bridge work must
  land upstream first and then be consumed through released crates.

## Current Support

- `Solver.solve(..., config=None)` and `SolverManager(config=None)` load a
  user-space `solver.toml` from the current directory when present. Explicit
  `SolverConfig` or `dict` configs are normalized before Rust handoff, and
  top-level plus phase-level termination fields match upstream SolverForge:
  `seconds_spent_limit`, `minutes_spent_limit`, `best_score_limit`,
  `step_count_limit`, `unimproved_step_count_limit`, and
  `unimproved_seconds_spent_limit`.
- Synchronous and retained scalar/list construction solves use upstream
  SolverForge. Dynamic scalar construction binds first-fit, cheapest insertion,
  and assignment-group first-fit or cheapest insertion phases when the
  construction phase declares `group_name`; dynamic list construction binds list
  cheapest insertion, list regret insertion, list Clarke-Wright, and list k-opt
  phases where the model supplies the required list or route hooks.
- `SolverManager` is backed by upstream retained jobs, statuses, events, and
  snapshots. `SolverManager.solve(...)` returns `JobHandle(job_id=...)`; the
  manager supports pause, resume, cancel, delete, exact snapshot reads, and
  retained event payloads whose current score is read from the attached solution
  snapshot when present.
- `planning_variable(...)` supports row candidate callbacks and nearby value or
  entity candidate/distance callbacks for dynamic scalar construction and nearby
  scalar local search.
- `scalar_assignment_group(...)` declares assignment-aware scalar groups for
  grouped scalar local search and assignment-group construction. Group metadata
  covers required entities, capacity keys, assignment rules, ordering callbacks,
  callback synchronization policy, and `ScalarGroupLimits`. Assignment callbacks
  receive a solution view whose entity/fact collections reflect Rust-owned
  planning state and whose ordinary solution-level attributes remain available
  for read-only lookup context such as capacity tables.
- `planning_list_variable(...)` supports `element_owner`,
  `construction_element_order_key`, `precedence_duration`,
  `precedence_successors`, route callbacks
  (`route_depot`, `route_metric_class`, `route_distance`, `route_feasible`),
  and entity-scoped route callbacks
  (`route_depot_entity`, `route_metric_class_entity`,
  `route_distance_entity`, `route_feasible_entity`) for owner-aware list moves,
  JSSP-style precedence/makespan scoring, and CVRP-style route construction.
  CVRP-style models with immutable row data can avoid per-query Python callbacks
  by passing
  `route_depot_field`, `route_metric_class_field`,
  `route_distance_matrix_field`, `route_capacity_field`, and
  `route_demand_field`; the Rust route phases read those fields through the
  active cursor and keep the callback options as fallbacks.
  `shadow_variable_updates(...)` registers post-update listeners whose
  native-owned fields are refreshed during solve/analyze and exported back to
  Python objects.
- Supported score families are `SoftScore`, `HardSoftScore`,
  `HardSoftDecimalScore`, and `HardMediumSoftScore`.
- Python callback constraints are evaluated from Rust-owned dynamic state.
  Supported stream shapes are unary `for_each(...).filter(...)`, binary
  `for_each(...).join(...).filter(...)`, and grouped-count
  `for_each(...).group_by(...)`, plus `for_each(...).balance(...)`,
  `for_each_unassigned_element(owner_entity_type, variable_name)`, and
  `list_precedence_makespan(owner_entity_type, variable_name)`. Ordinary
  penalize/reward streams support fixed or callback-computed score weights; list
  precedence/makespan scoring is computed natively from owner, duration, and
  successor callbacks. `indexed_presence(...)` is available as a grouped
  collector for run/range presence scoring. `joiner.equal(...)` and
  `joiner.equal_bi(...)` preserve Python equality semantics.
- Dynamic scalar local search is available through upstream-style dynamic
  `change_move_selector`, `swap_move_selector`, `nearby_change_move_selector`,
  `nearby_swap_move_selector`, `pillar_change_move_selector`,
  `pillar_swap_move_selector`, `ruin_recreate_move_selector`,
  `grouped_scalar_move_selector`, `conflict_repair_move_selector`, and
  `compound_conflict_repair_move_selector` phases.
- Dynamic list local search is available through upstream-style
  `list_change_move_selector`, `nearby_list_change_move_selector`,
  `list_swap_move_selector`, `nearby_list_swap_move_selector`,
  `sublist_change_move_selector`, `sublist_swap_move_selector`,
  `list_reverse_move_selector`, `list_permute_move_selector`,
  `list_precedence_move_selector`, `k_opt_move_selector`, and
  `list_ruin_move_selector` phases.
- `limited_neighborhood`, `union_move_selector`, and two-child
  `cartesian_product_move_selector` compose supported dynamic scalar and list
  selectors.
- `examples/solverforge_hospital` is a 1:1 Python model of the Rust hospital
  use case's public dataset/config surface: `LARGE` has 50 employees, 688
  shifts, retained jobs, snapshots, analysis, pause/resume/cancel, and a
  30-second hard-feasible terminal solve when installed with the release native
  extension.
- `examples/solverforge_deliveries` is the Python port of the deliveries use
  case: route-owning vehicles, delivery facts, CVRP route callbacks, route
  shadow metrics, unassigned-delivery scoring, retained jobs, route snapshots,
  analysis, and delivery-insertion recommendations.
- Top-level `ConstraintFactory.join`, `group_by`, `if_exists`, `if_not_exists`,
  and `flattened` remain explicit unsupported methods until the native callback
  stream planner has public bridge support for those top-level semantics. Use
  stream-level `for_each(...).join(...)` and `for_each(...).group_by(...)` for
  the supported join and grouped-count surfaces.

Rust macro-generated SolverForge models remain the performance ceiling. The
Python path preserves the Rust solver engine and Rust-owned state while paying
the honest dynamic callback boundary cost.

## Callback And Threading Contract

Python callbacks are the only constraint authoring surface. Filter callbacks
return `bool`; weight callbacks return a score object or integer; grouped scalar
callbacks return scalar edit candidates for `@scalar_group`; conflict repair
callbacks return scalar edit candidates for constraints declared with
`@conflict_repair`; value range providers return ordered finite collections;
distance callbacks return numeric distances; and owner hooks used by list
selectors return an owner index or `None`.

Callback exceptions are surfaced as SolverForge Python exceptions with the
original Python traceback. Callback solution views include non-private,
non-callable solution-level attributes from the imported Python solution, such as
lookup tables and value sets used by hooks. Entity and fact collections in those
views are projected from Rust-owned solver state so preview and best-solution
clones never share mutable Python row objects with the working solution.

The primary target is CPython 3.14 free-threaded. The native extension declares
that it does not require the GIL; Rust worker threads attach to Python only when
invoking Python callbacks. Callback code must be deterministic for a given
solution state, safe to run concurrently, and treat solution-level lookup context
as immutable for the duration of a solve. Third-party Python extension modules
used inside callbacks may still impose their own synchronization constraints.

## Release State

This repository publishes the `solverforge` package on PyPI. The current
checkout is `0.5.0`, carrying the SolverForge Rust dependency base forward to
`0.17.2`. The `v0.5.0` tag points at the source-current release head, GitHub CI
and Forgejo CI are green for that head, and the GitHub release workflow has built
and verified the source distribution plus Linux, macOS, and Windows wheels. As
of July 4, 2026, the release workflow is waiting at the reviewed `pypi`
environment before publishing.

PyPI latest for `solverforge` is still `0.4.0`, with published versions `0.2.2`,
`0.2.3`, `0.2.4`, `0.2.5`, `0.2.6`, `0.3.0`, and `0.4.0`; TestPyPI also has
`0.4.0`. The old PyPI artifacts describe a different API and architecture,
including `SolverFactory`, `PlanningVariable`, and Java-service requirements.

Release rules:

- Publish the verified final `0.5.0`, not a prerelease.
- Keep `requires-python = ">=3.14"`.
- Build wheels and the source distribution from this repository, with the
  SolverForge Rust dependency base declared in `Cargo.toml` and locked in
  `Cargo.lock`.
- Publish to TestPyPI only from `workflow_dispatch` through trusted publishing.
  Tagged `v*.*.*` pushes build and verify release artifacts, then publish to
  PyPI only after the reviewed `pypi` environment approves the job.
- After PyPI publication, verify `python3.14 -m pip install solverforge`
  resolves to `0.5.0`.
- After `0.5.0` is published and smoke-tested, yank PyPI `0.2.2` through
  `0.3.0` with the reason
  `Superseded by solverforge 0.5.0 dynamic Python binding architecture.`

Trusted publishing should trust owner `SolverForge`, repository
`solverforge-py`, workflow `release.yml`, TestPyPI environment `testpypi`, and
PyPI environment `pypi`. The `pypi` GitHub environment should require manual
approval, and the workflow uses OIDC rather than long-lived PyPI tokens. Run
`make pre-release` before publishing; it checks the SolverForge dependency base,
runs local CI, builds release artifacts, runs `twine check`, and verifies
artifact contents.

## License

SolverForge Python is licensed under the Apache License, Version 2.0. See
`LICENSE` for the full license text.
