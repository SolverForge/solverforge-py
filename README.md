# SolverForge Python

`solverforge` is the dynamic Python binding package for SolverForge. Python
users define models with Python classes, decorators, functions, and lambdas.
They do not write Rust and they do not pass JSON to a fixed backend.

The native extension owns the working solution state in Rust so SolverForge can
clone, mutate, and snapshot solutions safely. Python callbacks are the single
constraint authoring surface.

The package targets CPython 3.14 and Rust 1.95.0. The current published
`solverforge` `0.6.6` package pins the six published SolverForge Rust
dependencies to `0.19.4` and `solverforge-ui` to `0.7.0` through exact
crates.io requirements in `Cargo.toml` and registry checksums in `Cargo.lock`.
There is no local path or Git dependency override. The `solverforge` `0.6.x`
line is the current dynamic binding architecture and intentionally supersedes
the older incompatible `0.2.x` and `0.3.0` artifacts in the same PyPI namespace.
Those older artifacts exposed `SolverFactory`, `PlanningVariable`, Java service
requirements, and other APIs that are not part of this package.

## Installation

The current published Python package is `solverforge` `0.6.6`; the native
`solverforge_py` crate metadata shares that version and targets the published
and registry-locked SolverForge Rust `0.19.4` dependency base. Install it with:

```sh
python3.14 -m pip install "solverforge==0.6.6"
```

The installable wheel contains the core `solverforge` package, native extension,
and embedded shared `solverforge-ui` assets exposed through
`solverforge.ui.asset()` and `solverforge.ui.asset_paths()`.
Source-checkout examples, including the hospital and deliveries FastAPI apps and
their app-specific static files, are maintained in this repository rather than
installed into the runtime wheel.

## Examples

Run source-current examples through the root Makefile:

```sh
make hospital-run
make deliveries-run PORT=7861
```

The same source-checkout examples can be run against the published package
without `PYTHONPATH` or an editable checkout:

```sh
python3.14 -m venv .venv-examples
. .venv-examples/bin/activate
python -m pip install "solverforge[examples]==0.6.6"
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
- Public bridge work belongs upstream; the wrapper consumes only public seams
  rather than private upstream modules or a wrapper-owned solver path.

## Current Support

- `Solver.solve(..., config=None)` and `SolverManager(config=None)` load a
  user-space `solver.toml` from the current directory when present. Explicit
  `SolverConfig` or `dict` configs are normalized before Rust handoff, and
  top-level plus phase-level termination fields match upstream SolverForge:
  `seconds_spent_limit`, `minutes_spent_limit`, `best_score_limit`,
  `step_count_limit`, `unimproved_step_count_limit`, and
  `unimproved_seconds_spent_limit`.
- Synchronous and retained scalar/list construction solves use one upstream
  compiled runtime runner. Ordinary dynamic scalar construction supports
  `first_fit` and `cheapest_insertion`. A declared assignment group additionally
  supports `first_fit_decreasing`, `weakest_fit`, `weakest_fit_decreasing`,
  `strongest_fit`, and `strongest_fit_decreasing`; the decreasing variants
  require `entity_order`, while weakest/strongest variants require
  `value_order`. Dynamic list construction supports `list_round_robin`,
  `list_cheapest_insertion`, `list_regret_insertion`, `list_clarke_wright`, and
  `list_k_opt`, with the final two requiring their respective savings and route
  metadata bundles. These are core graph nodes over the immutable runtime
  model. All configured limits remain binding during construction. A solve
  publishes a best or completed solution only after every list element,
  required assignment row, and non-optional scalar variable is assigned; if a
  limit is reached first, a direct solve raises and a retained job enters
  `FAILED` without exposing a partial snapshot. Omitted construction still
  resolves required and optional stages in core, but required work never
  bypasses a configured limit. Default local search is assembled only when the top-level
  termination has an effective finite limit, so an empty or invalid termination
  cannot accidentally create an unbounded solve. There is no wrapper assignment
  placer, required stream, phase tree, TLS slot state, synthetic metric, or
  fallback path.
- `SolverManager` is backed by upstream retained jobs, statuses, events, and
  snapshots. `SolverManager.solve(...)` returns `JobHandle(job_id=...)`; the
  manager supports pause, resume, cancel, delete, and exact snapshot reads.
  A pause before mandatory construction completes remains resumable internal
  state and does not expose an incomplete solution snapshot.
  Retained event payloads read current score from the attached solution snapshot
  when present. Long-running phases publish progress at the shared upstream
  cadence, and telemetry includes the current phase type, index, counters, and
  generation/evaluation time. `telemetry_detail(job_id)` returns one atomic
  `status` with detailed telemetry plus an optional `candidate_trace`; ordinary
  status/event polling remains compact.
- Candidate tracing is an opt-in retained-job diagnostic configured as
  `candidate_trace = { max_entries = N }`, where `N` must be non-zero. The core
  records the total pull count after the bounded identity prefix is full and
  marks truncation explicitly. Synchronous `Solver.solve(...)` rejects
  `candidate_trace` because only `SolverManager.telemetry_detail(...)` can return
  it. Trace format 3 exposes canonical configured input, execution policy,
  resolved phase-plan provenance, candidate identities and dispositions, prefix
  digests, completeness flags, and the explicit
  `candidate_index_scope = "source_local_only"` contract.
  `QualifiedCandidateTraceProvenance(...)` accepts keyword-only lowercase
  SHA-256 values for the schema, instance, initial state, core tree, and loaded
  build plus a non-blank external producer. Pass it per job as
  `SolverManager.solve(..., qualified_candidate_trace_provenance=...)`; it is
  rejected before schema discovery unless that manager has candidate tracing
  enabled. The provenance is never inferred from a solution or accepted through
  serializable solver config, and ordinary traces remain explicitly
  `not_requested` rather than being upgraded by the presence of digest fields.
- `@candidate_metric("name")` registers a named numeric move-ranking callback
  through `@planning_solution(..., candidate_metrics=[...])`. A metric receives
  a read-only solution callback view and the core candidate's canonical logical
  identity. Leaf selectors with `selection_order = "sorted"` or
  `"probabilistic"` must name a registered `selection_metric`; other selection
  orders reject that field. Sorted metrics are ascending. Metric values must be
  finite, and probabilistic weights must also be non-negative, with zero-weight
  candidates omitted.
- `planning_variable(...)` supports row candidate callbacks and nearby value or
  entity candidate/distance metadata for dynamic scalar construction and nearby
  scalar local search. Each nearby metadata parameter has exactly one source:
  either a Python callback or a row field name. Dual raw-schema sources are
  rejected at compilation, while immutable row metadata can be supplied without
  per-query Python callbacks. Provider-backed value ranges are imported once per
  variable and shared across rows in Rust-owned state. Row candidate callbacks
  remain row-specific and define move legality throughout construction and local
  search, including assignment-group swaps and rematches.
- `scalar_assignment_group(...)` declares assignment-aware scalar groups for
  grouped scalar local search and assignment-group construction. Group metadata
  covers required entities, capacity keys, assignment rules, ordering callbacks,
  callback synchronization policy, and `ScalarGroupLimits`. Assignment callbacks
  receive a solution view whose entity/fact collections reflect Rust-owned
  planning state and whose ordinary solution-level attributes remain available
  for read-only lookup context such as capacity tables. For invariant row
  metadata, `required_entity_field`, `capacity_key_field`,
  `position_key_field`, and `sequence_key_field` are explicit alternatives to
  their callbacks: required is a row bool, position and sequence are row
  integers, and capacity is a row list indexed by candidate value.
  `same_value_conflict_field` is the native alternative for a static
  `assignment_rule`: each row lists adjacent-sequence entity indexes that
  cannot hold the same assigned value, and the group still requires sequence
  metadata. Field alternatives avoid per-candidate Python transitions without
  caching or changing callback behavior.
  The compiled schema retains one immutable runtime plan (schema, descriptor,
  and model), reused by direct solves, manager jobs, snapshots, and resumes;
  instance rows, callback views, seeds, and moves remain per solve.
  An assignment-owned planning variable is exclusively edited through its
  declared grouped construction and `grouped_scalar_move_selector`; a raw
  scalar, nearby, ruin, or conflict-repair selector targeting that same
  variable is rejected instead of creating a second search path. Generic
  `@scalar_group` and conflict-repair callbacks are rejected in a local-search
  phase that could reach an assignment-owned slot: their returned edits are
  unscoped, so accepting them would recreate a second ownership path. Multiple
  declarative assignment groups can still compose in selector combinators.
- `planning_list_variable(...)` supports `element_owner`,
  `construction_element_order_key`, `precedence_duration`, and
  `precedence_successors` for owner-aware list moves and JSSP-style
  precedence/makespan scoring. Its route-related metadata has one canonical,
  nested declaration form: `route=ListRouteHooks(...)`,
  `savings=ListSavingsHooks(...)`, plus independent
  `cross_position_distance=` and `intra_position_distance=` sources. Route and
  savings are complete, independent bundles: a route bundle never implicitly
  enables savings construction.
  The owner, construction-order, duration, and successor metadata parameters
  each accept either a Python callback or the name of a solution-level sequence
  indexed by element ID; string sources let the native runtime read immutable
  element metadata without per-element Python calls. A declared sequence is
  mandatory before state import: owner entries are `None` or non-negative
  integers, order entries are integers, durations are non-negative integers,
  and successor entries are ordered sequences of non-negative integers. Missing
  or malformed sequences fail import; they never become an unrestricted owner,
  default order, or missing precedence edge. Nested route sources use explicit
  source wrappers rather than unscoped strings or callbacks.
  In a nested list bundle, `RowField("...")`, `SolutionField("...")`,
  `EntityCallback(...)`, and `SolutionCallback(...)` make the source scope
  explicit. `RowField` reads only the declared entity row; `SolutionField` reads
  only the declared immutable solution-root value. Neither falls back to the
  other scope. Capacity feasibility is an explicit
  `CapacityRouteFeasibility(capacity=..., demand=...)` source whose two fields
  are independently scoped. A new nested declaration therefore never infers
  callback scope from arity or shares a route source with savings.

  Nearby list neighborhoods require their own
  `cross_position_distance` and `intra_position_distance` sources, and neither
  is inferred from a route distance. A matrix source for either metric is
  indexed by the actual list values at the two positions, never by their row or
  position indexes.
  `shadow_variable_updates(...)` registers post-update listeners whose
  native-owned fields are refreshed during solve/analyze and exported back to
  Python objects. Attached callback solution views synchronize dirty rows after
  the first full view sync instead of rewriting every entity row for repeated
  full-solution callbacks.
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
  successor metadata callbacks or field-name metadata. `indexed_presence(...)`
  is available as a grouped collector for run/range presence scoring.
  `joiner.equal(...)` and `joiner.equal_bi(...)` accept either Python key
  callbacks or attribute-name strings. Callback keys preserve Python equality
  semantics. String keys specialize to native equality only for planning scalar
  slots and stable fields stored directly on every imported row; properties,
  shadow-derived attributes, containers, and unsupported scalar values use live
  Python attribute lookup and equality.
  Schema/runtime plans reuse only capture-free functions with canonical module
  namespace provenance and no defaults or function-owned metadata. A callback
  from a different module namespace never shares a plan merely because its code
  object matches; mutable values in the same callback namespace remain live.
  Closures, bound defaults, partials, methods, callable instances, and other
  stateful callbacks compile per invocation rather than being retained across solves.
  Constraint plans specialize after state import; simple fixed-weight unary,
  list-unassigned, and proven string-key equality joins can evaluate directly
  from Rust-owned state while callback filters, callback weights, computed
  attributes, and unsupported key values use the Python callback surface.
- Dynamic scalar local search is available through upstream-style dynamic
  `change_move_selector`, `swap_move_selector`, `nearby_change_move_selector`,
  `nearby_swap_move_selector`, `pillar_change_move_selector`,
  `pillar_swap_move_selector`, `ruin_recreate_move_selector`,
  `grouped_scalar_move_selector`, `conflict_repair_move_selector`, and
  `compound_conflict_repair_move_selector` phases.
  Raw scalar selectors exclude variables owned by `scalar_assignment_group(...)`;
  use that group's `grouped_scalar_move_selector` for its assignment variable.
- Dynamic list local search is available through upstream-style
  `list_change_move_selector`, `nearby_list_change_move_selector`,
  `list_swap_move_selector`, `nearby_list_swap_move_selector`,
  `sublist_change_move_selector`, `sublist_swap_move_selector`,
  `list_reverse_move_selector`, `list_permute_move_selector`,
  `list_precedence_move_selector`, `k_opt_move_selector`, and
  `list_ruin_move_selector` phases.
- `limited_neighborhood`, `union_move_selector`, and two-child
  `cartesian_product_move_selector` compose supported dynamic scalar and list
  selectors. Dynamic neighborhoods are opened as resumable cursor trees: union
  children, limits, Cartesian branches, pillar windows, and k-opt reconnections
  advance only when the solver requests the next candidate. Nearby candidate
  callbacks may return any Python iterable; candidates are consumed once into
  an exact bounded top-k rather than copied into a full ranked neighborhood.
  Stable candidate IDs remain borrowable for evaluation, losing candidates are
  released as soon as the forager no longer needs them, and only the selected
  move crosses the ownership boundary. Exact sorting, shuffling, and probability
  decorators retain the compact global ordering data their documented semantics
  require.
- `examples/solverforge_hospital` is a 1:1 Python model of the Rust hospital
  use case's public dataset/config surface: `LARGE` has 50 employees, 688
  initially unassigned shifts, skill/availability-aware construction candidates,
  retained jobs, snapshots, analysis, pause/resume/cancel, and a reproducible
  30-second hard-feasible terminal solve when installed with the release native
  extension.
- `examples/solverforge_deliveries` is the Python port of the deliveries use
  case: initially unassigned delivery facts, route-owning vehicles, independent
  row-backed `ListRouteHooks` and `ListSavingsHooks` bundles with
  solution-scoped feasibility, route shadow metrics, unassigned-delivery
  scoring, retained jobs, route snapshots, analysis, and delivery-insertion
  recommendations.
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
`@conflict_repair`; candidate-metric callbacks return finite numeric ranking
values; value range providers return ordered finite collections; nearby
candidate callbacks return Python iterables; distance callbacks return numeric
distances; and owner hooks used by list selectors return an owner index or
`None`. Candidate metrics receive the callback solution view plus an operation
or composite identity dictionary; they rank an already generated core move and
do not rebuild or execute a second selector.

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

## License

SolverForge Python is licensed under the Apache License, Version 2.0. See
`LICENSE` for the full license text.
