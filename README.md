# SolverForge Python

`solverforge` is the dynamic Python binding package for SolverForge. Python
users define models with Python classes, decorators, functions, and lambdas.
They do not write Rust and they do not pass JSON to a fixed backend.

The native extension owns the working solution state in Rust so SolverForge can
clone, mutate, and snapshot solutions safely. Python callbacks are the single
constraint authoring surface.

The package targets CPython 3.14 and Rust 1.95.0. The PyPI package starts at
`solverforge` `0.4.1` for this architecture and intentionally supersedes the
older incompatible `0.2.x` and `0.3.0` artifacts in the same PyPI namespace.
Those older artifacts exposed `SolverFactory`, `PlanningVariable`, Java service
requirements, and other APIs that are not part of this package.

## Installation

```sh
python3.14 -m pip install solverforge
```

The installable wheel contains the core `solverforge` package, native extension,
and embedded shared `solverforge-ui` assets exposed through `solverforge.ui`.
Source-checkout examples, including the hospital and deliveries FastAPI apps and
their app-specific static files, are maintained in this repository rather than
installed into the runtime wheel.

## Examples

Run source-checkout examples against the published PyPI package. The examples
import `solverforge` from the installed package and do not require `PYTHONPATH`
or an editable local checkout.

```sh
python3.14 -m venv .venv-examples
. .venv-examples/bin/activate
python -m pip install "solverforge[examples]==0.4.1"
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
make test             # cargo test plus pytest
make lint             # rustfmt check, ruff, mypy, and clippy
make ci-local         # local CI simulation
make pre-release      # ci-local plus release sdist/wheel checks
```

Run `make help` for focused targets such as `make test-hospital`,
`make test-deliveries`, `make test-one TEST=pattern`, `make hospital-run`,
`make hospital-solve`, `make deliveries-run`, and `make deliveries-solve`.

## Boundaries

- No generated Rust.
- No expression-object DSL.
- No string-parsed constraints.
- No private upstream SolverForge modules; bindings use the public dynamic bridge
  contract.
- No fixed pre-modeled JSON-only backends.

## Current Support

- `Solver.solve(..., config=None)` and `SolverManager(config=None)` load a
  user-space `solver.toml` from the current directory when present. Explicit
  `SolverConfig` or `dict` configs are normalized before Rust handoff, and
  top-level plus phase-level termination fields match upstream SolverForge:
  `seconds_spent_limit`, `minutes_spent_limit`, `best_score_limit`,
  `step_count_limit`, `unimproved_step_count_limit`, and
  `unimproved_seconds_spent_limit`.
- Synchronous and retained scalar/list construction solves use upstream
  SolverForge. Dynamic scalar construction binds first-fit and cheapest
  insertion phases; dynamic list construction binds list cheapest insertion,
  list Clarke-Wright, and list k-opt phases where the model supplies the
  required list or route hooks.
- `SolverManager` is backed by upstream retained jobs, statuses, events, and
  snapshots, including pause, resume, cancel, delete, and exact snapshot reads.
- `planning_list_variable(...)` supports `element_owner`, `route_depot`,
  `route_metric_class`, `route_distance`, and `route_feasible` callbacks for
  owner-aware list moves and CVRP-style route construction.
  `shadow_variable_updates(...)` registers post-update listeners whose
  native-owned fields are refreshed during solve/analyze and exported back to
  Python objects.
- Supported score families are `SoftScore`, `HardSoftScore`,
  `HardSoftDecimalScore`, and `HardMediumSoftScore`.
- Python callback constraints are evaluated from Rust-owned dynamic state.
  Supported stream shapes are unary `for_each(...).filter(...)`, binary
  `for_each(...).join(...).filter(...)`, and grouped-count
  `for_each(...).group_by(...)`, plus `for_each(...).balance(...)` and
  `for_each_unassigned_element(owner_entity_type, variable_name)`, with fixed
  or callback-computed score weights. `joiner.equal(...)` and
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
  `list_reverse_move_selector`, `k_opt_move_selector`, and
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

## Documentation Map

- `WIREFRAME.md` is the as-built public API, runtime, and example UI/API map.
- `AGENTS.md` is the contributor guide and agent scope contract.
- `docs/upstream-contract.md` records the public SolverForge bridge assumptions.
- `docs/dynamic-move-parity-plan.md` tracks implemented dynamic selector parity.

Rust macro-generated SolverForge models remain the performance ceiling. The
Python path preserves the Rust solver engine and Rust-owned state while paying
the honest dynamic callback boundary cost.

## License

SolverForge Python is licensed under the Apache License, Version 2.0. See
`LICENSE` for the full license text.
