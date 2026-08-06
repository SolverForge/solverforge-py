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
  generated UI model, seeded city data, row-backed CVRP route/savings metadata,
  route snapshots, retained job lifecycle, and insertion recommendations.
- Shared `/sf/*` frontend assets are embedded by the `solverforge-ui` Rust crate
  and exposed to Python through `solverforge.ui`, not copied into each example.
  Versioned `/sf/*` asset names are served only when the pinned `solverforge-ui`
  crate owns that exact file; stale versioned asset requests fail instead of
  aliasing to current bytes.
- Current contract documentation is intentionally kept in `README.md`,
  `AGENTS.md`, this wireframe, and the example READMEs. `CHANGELOG.md` remains
  tool-managed release history; there is no separate `docs/` directory.

## Python Package API

The package exports `Solver`, `SolverManager`, `JobHandle`, `SolverConfig`,
`TerminationConfig`, `QualifiedCandidateTraceProvenance`, `ConstraintFactory`,
`SoftScore`, `HardSoftScore`, `HardSoftDecimalScore`, `HardMediumSoftScore`,
stable package error classes (`SolverForgeError`, `CallbackError`,
`ModelValidationError`, and `NativeBridgeError`), `joiner`, `console`, `ui`, and
the `__version__` string, plus decorators/helpers for model authoring:

- `@planning_solution`, `@planning_entity`, `@problem_fact`
- `planning_id`, `planning_variable`, `planning_list_variable`
- `@constraint_provider`, `@scalar_group`, `@conflict_repair`, `@candidate_metric`
- `scalar_assignment_group(...)`, `ScalarAssignmentGroup`, `ScalarGroupLimits`
- `RowField`, `SolutionField`, `EntityCallback`, `SolutionCallback`
- `ListMetadata`, `ListRouteHooks`, `ListSavingsHooks`,
  `CapacityRouteFeasibility`
- `shadow_variable_updates(...)`
- `indexed_presence(...)`

`console.init()` initializes the upstream Rust console subscriber. The public
`ui` module exposes `asset(path) -> UiAsset | None` and `asset_paths() ->
list[str]`; each `UiAsset` carries its canonical path, content type, cache
control value, and bytes.

`@candidate_metric("name")` functions are registered by
`@planning_solution(..., candidate_metrics=[...])`. The callback receives a
read-only solution view and one canonical logical candidate dictionary. An
operation identity contains `type`, `operation`, `descriptor_index`,
`entity_class`, `variable_name`, and raw `coordinates`; a composite identity
contains `type`, `operation`, and recursive `children`. A leaf selector may name
the metric through `selection_metric` only when `selection_order` is `sorted` or
`probabilistic`. Sorted values are ascending; all values must be finite, and
probabilistic weights must be non-negative.

Solutions are normal Python objects. Entity and fact collections are inferred
from type hints where available, then from instance lists. The native module
mutates Rust-owned dynamic state and exports the solved state back to Python.
Python callback solution views preserve ordinary solution-level lookup context
while projecting entity/fact collections from Rust-owned state, so preview clones
do not share mutable row objects with the working solution. Attached callback
views perform one full row synchronization, then synchronize only dirty entity
rows for repeated full-solution callbacks.
Scalar variables may declare each nearby candidate or distance source as either
a Python callback or a row field name for nearby scalar construction and local
search; a source cannot be both. Raw schemas that supply both forms are rejected
at compilation. Shared value-range providers are stored once per variable in
Rust-owned state and row candidate callbacks remain row-specific.
Assignment-aware scalar groups may likewise declare invariant required,
capacity, position, and sequence metadata as row fields instead of callbacks.
Those field sources are read from Rust-owned rows on every use; they do not add
a result cache or alter the delivery of any remaining Python callbacks.
List variables may declare element-owner, construction-order, precedence
duration, and precedence-successor metadata with either Python callbacks or
solution-level sequence names indexed by element ID. Route-related metadata is
one canonical nested schema object: independent complete
`ListRouteHooks` and `ListSavingsHooks` bundles, plus independent
`cross_position_distance` and `intra_position_distance` sources. A route bundle
never implies a savings bundle.

Declared solution-level element metadata sequences are validated before dynamic
state construction: owner entries are `None` or non-negative integers, order
entries are integers, durations are non-negative integers, and successor
entries are ordered sequences of non-negative integers. A missing or malformed
sequence is an import error, never an implicit unrestricted owner, default
order, or missing precedence edge. Callback metadata remains lazy.

`RowField`, `SolutionField`, `EntityCallback`, and `SolutionCallback` make each
nested source scope explicit. A row source never falls back to a same-named
solution field; a `SolutionField` is a separately declared immutable
solution-root source. `CapacityRouteFeasibility` explicitly carries its capacity
and demand field sources. Nearby list change/swap and K-opt neighborhoods use
their own declared position metrics; those matrix sources use the actual values
stored at the requested positions, never entity or position indexes.

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
  `for_each_unassigned_element(owner_entity_type, variable_name)` followed by
  `.filter(...).penalize/reward(...).named(...)`
- list precedence/makespan
  `list_precedence_makespan(owner_entity_type, variable_name).named(...)`

Ordinary penalize/reward stream weights may be score objects, integer/sequence
values, or Python callbacks. List precedence/makespan scoring is computed
natively from owner, duration, and successor metadata declared on the planning
list variable.
Grouped streams support the default count collector and `indexed_presence(...)`
for run/range presence analysis.
`joiner.equal(...)` and `joiner.equal_bi(...)` accept either Python key
callbacks or attribute-name strings. Callback keys preserve Python equality
semantics. String keys specialize to native equality for planning scalar slots
and stable instance fields present on every imported row. Computed properties,
shadow-derived attributes, containers, and unsupported scalar values use live
Python attribute lookup and equality.
Schema/runtime plans reuse only capture-free functions with canonical module
namespace provenance and no defaults or function-owned metadata. A callback
from a different module namespace never shares a plan merely because its code
object matches; mutable values in the same callback namespace remain live.
Closures, bound defaults, partial bindings, methods, callable instances, and
other stateful callbacks compile per invocation rather than being retained across solves.
Constraint plans specialize after state import; simple fixed-weight unary,
list-unassigned, and proven string-key equality joins can evaluate directly from
Rust-owned state, while callback filters, callback weights, computed attributes,
and unsupported key values use the Python callback path. Top-level
`ConstraintFactory.join`, `group_by`, `if_exists`, `if_not_exists`, and
`flattened` remain explicit unsupported methods.
Stream-level `for_each(...).join(...)` and
`for_each(...).group_by(...)` are the supported join and grouped-count surfaces.

## Solver And Runtime Flow

`Solver.solve(solution, config=None)` builds a schema, imports Python data into
Rust dynamic state, builds the upstream dynamic runtime model, runs SolverForge,
and writes assignments plus score back onto the original solution. It rejects a
`candidate_trace` configuration because the synchronous API has no retained
diagnostic detail endpoint.

`Solver.analyze(solution)` imports Python data, refreshes native-owned shadow
fields, evaluates callback constraints against the imported state, exports
refreshed assignments/shadow fields back to the Python object, and writes the
calculated score to `solution.score`.

`SolverManager(config=None)` wraps upstream retained jobs. It exposes `solve`,
which returns `JobHandle(job_id=...)`, plus `get_status`, `telemetry_detail`,
`events`, `wait`, `snapshot`, `pause`, `resume`, `cancel`, and `delete`.
`snapshot(job_id, snapshot_revision=None)` may request the latest or one exact
retained revision.
Submitted Python solutions are deep-copied before import so retained jobs do not
mutate the caller's object. Exact snapshots are deep-copied Python solutions
exported from Rust-owned retained state. Example app bootstrap and retained
event payloads report the current score from the attached solution snapshot when
one is available, and use that score as the best-score fallback only when the
retained manager event has not supplied a best score.

Retained progress uses the upstream phase pulse and carries nested phase
telemetry (`phase_index`, `phase_type`, elapsed time, move/step/score counters,
and generation/evaluation time). The hospital and deliveries DTOs map that
single native payload to their camel-case SSE/API contract. Failed events carry
the formatted Python callback traceback; synchronous solve/analyze re-raise the
original Python exception object and traceback. `telemetry_detail` is an
explicit atomic diagnostic read containing one status with detailed telemetry
and an optional candidate trace, without bloating normal status/event polling.

Candidate tracing is enabled only by a non-zero bounded
`candidate_trace.max_entries` configuration. The returned format-3 dictionary
contains the canonical configured input, execution policy, resolved phase plan,
their digests and completeness flags, an optional qualified provenance block,
the bounded pull prefix, total pull count, truncation state, prefix digest, and
explicit provenance status. Each pull carries a global ordinal, source, phase,
step and selector coordinates, a source-local `candidate_index`, a canonical
operation/composite identity when encodable, and its dispositions. Therefore
cross-runtime consumers key on ordinal plus identity; the exported
`candidate_index_scope` is exactly `source_local_only`.

`QualifiedCandidateTraceProvenance` is an immutable, keyword-only external
attestation with `schema_sha256`, `instance_sha256`, `initial_state_sha256`,
`core_tree_sha256`, `build_sha256`, and `producer`. Every digest must be exactly
64 lowercase hexadecimal characters and the producer must be non-blank. The
value is passed only as the per-job
`SolverManager.solve(..., qualified_candidate_trace_provenance=...)` argument.
It is rejected before schema discovery and deepcopy unless the manager was
configured with candidate tracing. `SolverConfig` recursively rejects that field
so a TOML or reusable dict cannot claim provenance for a different imported
instance. Ordinary candidate traces retain explicit `not_requested`
qualification.

Config may be a `SolverConfig`, a dict, or `None`. `SolverConfig` exposes
`from_dict`, `from_toml`, `load`/`from_file`, and `to_dict`; `TerminationConfig`
exposes `from_dict` and `to_dict`. When solve or manager config is `None`,
`solver.toml` in the current directory is loaded if present. Termination fields
are accepted at the top level or under `termination`, and phase termination is
normalized before handoff.

## Upstream Boundary

The binding works against public upstream crates only. `PlanningSolution`
requires `Clone + Send + Sync + 'static`. `ConstraintSet<S, Score>` is the
public scoring seam used by the dynamic adapter. Logical descriptor IDs and
dynamic backend/slot contracts are exposed through `solverforge-core`, with
`solverforge-bridge` re-exporting them for binding crates, so dynamic Python
entity classes do not need distinct Rust row types for descriptor identity.

Upstream exposes a model-only compiled-runner bridge, allowing the binding to
pass an already-built descriptor, constraint set, immutable runtime model,
config, and runtime values without a macro-style factory or a binding-supplied
phase builder.
`SolverRuntime::detached()` is used for synchronous binding solves that should
use the upstream runner without copying retained manager internals. Retained
Python jobs use upstream `SolverManager<PyDynamicSolution>` and the same
`Solvable`/runtime path as synchronous dynamic solves.

Rust-only custom search and partitioner registration remain Rust-only unless
upstream exposes a public host-language binding seam.

## Dynamic Move Support

Dynamic Python models support the public upstream move selectors listed below
without exposing Rust macros, generating Rust, or silently degrading one
selector into another. They lower into the same compiled scalar/list runtime
leaves as native models; Python supplies schema metadata and callbacks, not a
second move family or selector executor.

Dynamic construction phases:

- scalar `first_fit` and `cheapest_insertion`
- scalar assignment-group `first_fit`, `first_fit_decreasing`, `weakest_fit`,
  `weakest_fit_decreasing`, `strongest_fit`, `strongest_fit_decreasing`, and
  `cheapest_insertion` when a construction phase declares `group_name`;
  decreasing variants require `entity_order`, and weakest/strongest variants
  require `value_order`
- list `list_round_robin`, `list_cheapest_insertion`, and
  `list_regret_insertion`
- route-aware list `list_clarke_wright` and `list_k_opt`

Dynamic scalar selectors:

- `change_move_selector`, `swap_move_selector`
- `nearby_change_move_selector`, `nearby_swap_move_selector`
- `pillar_change_move_selector`, `pillar_swap_move_selector`
- `ruin_recreate_move_selector`
- `grouped_scalar_move_selector`
- `conflict_repair_move_selector`
- `compound_conflict_repair_move_selector`

An assignment-owned scalar is represented by exactly one canonical grouped
runtime binding. Raw scalar, nearby, ruin, and conflict-repair selectors may
target other dynamic scalar variables but are rejected for that binding; unions
and cartesian products may compose multiple declared grouped selectors.

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

The compiled core selector tree is cursor-driven rather than arena-built.
Scalar and list leaves yield one move at a time; union and limited selectors do
not open unreached children; Cartesian selectors retain one reversible left
preview; pillar selectors retain grouped range metadata; and k-opt enumerates
cuts and reconnections incrementally. Nearby row metadata, configured value
ranges, and Python iterables stream into an exact bounded top-k.
Grouped/conflict callbacks keep their explicitly bounded finite-batch contract.

The public upstream cursor contract borrows candidates by stable ID, supports
direct owned production for streaming consumers, releases losing candidates,
and transfers exact ownership of only the selected move. Core construction uses
one live entity-placer cursor. Assignment scalar groups are compiled into the
canonical upstream `ScalarGroupBinding` and use that same upstream
grouped-construction cursor and grouped selector; the wrapper owns no
assignment cursor, required stream, phase, or fallback. Explicit group phases
obey their configured limits. The compiled runtime withholds best/completed
solutions and snapshots until every list element, required assignment row, and
non-optional scalar variable is assigned. Reaching a configured limit first
raises from a direct solve or uses the retained `FAILED` lifecycle; a pause
before completion remains resumable without exposing a partial snapshot.
Omitted construction still owns required/optional stage resolution, but no
required work bypasses configured termination. Default local search is assembled only when the
top-level termination has an effective finite limit. Exact global decorators
retain only the candidate/index metadata required to preserve their documented
ordering, seeded randomness, and probability semantics.

The dynamic runtime is one immutable `RuntimeModel` compiled and executed by
upstream SolverForge. Its scalar, list, grouped-assignment, cartesian, nearby,
and K-opt selectors are core runtime nodes, not wrapper move types. Nearby
selectors require the declared cross/intra position metadata that their core
leaf consumes; the binding never substitutes an ordinary candidate universe or
a synthetic metric.

Callback scalar groups and conflict repair selectors use Python-declared
callbacks that return compound scalar edit candidates. Assignment scalar groups
instead compile declarative metadata into the canonical SolverForge runtime.
When a model has an assignment-owned scalar, a wrapper-local generic group or
repair selector is rejected if it could reach that slot; callback edits are
otherwise unscoped and would create a second mutation path. Raw dynamic scalar
selectors are likewise rejected for the assignment member. Declarative
assignment groups can compose only through canonical grouped selectors.
Rust owns the actual solver state and applies dynamic edits through its runtime
slots, so clone, snapshot, move mutation, construction, and callback ordering
semantics remain Rust-side.

Implementation status is complete across the relevant surfaces:
`python/solverforge/decorators.py` exposes `@scalar_group`,
`@conflict_repair`, `@candidate_metric`, and
`@planning_solution(..., scalar_groups=..., conflict_repairs=...,
candidate_metrics=...)`; `python/solverforge/model.py` emits and validates those
schema registries. The Rust schema compiles them into one immutable runtime plan
containing the schema, descriptor, upstream `RuntimeModel`, assignment bindings,
and candidate-metric registry. `src/runtime/{scalar_slots,list_slots}.rs` is
limited to Rust-owned dynamic state and metadata access, while
`src/runtime/candidate_metric.rs` adapts the declared Python ranking callbacks.
`src/solver/{api,solvable}.rs` transfers that model to the model-only bridge.
There is no wrapper construction phase, selector tree, TLS active-slot stack,
synthetic distance meter, or dynamic move type.

## Hospital Example UI/API

`examples/solverforge_hospital` mirrors the Rust `uc-hospital/src` ownership
tree with `domain/`, `constraints/`, `data/data_seed/`, `solver/service/`, and
`api/`. Its `LARGE` fixture contains 50 employees and 688 initially unassigned
shifts. `Shift.employee_idx` uses a row candidate callback to remove employees
without the required skill or with overlapping unavailability before
construction, then row-backed nearby candidates/distances plus the shift
distance callback for nearby search. The reproducible seed-1 config uses
30-second/5-second-unimproved termination, cheapest insertion with
`assign_when_candidate_exists`, and max-10 nearby change/swap under late
acceptance with a first-best-score-improving forager. It exposes:

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

The hospital app is source-checkout example material. It remains available in
the repository for development, but it is intentionally excluded from both the
build-only source distribution and the runtime wheel.

## Deliveries Example UI/API

`examples/solverforge_deliveries` mirrors the Rust `uc-deliveries/src`
ownership tree with `domain/`, `constraints/`, `data/data_seed/`,
`solver/service/`, and `api/`. It models deliveries as facts and vehicles as
route-owning planning entities with `Vehicle.delivery_order` as a planning list
variable. Seed routes begin empty. The list variable declares independent
`ListRouteHooks` and `ListSavingsHooks`: depot, per-vehicle metric class, and the
per-vehicle distance matrix are explicit `RowField` sources, while route
feasibility is a `SolutionCallback`. Route metric shadows are refreshed through
`shadow_variable_updates(...)`. The reproducible seed-42 config uses a
three-second solve, list cheapest insertion, list k-opt construction polish with
`k = 2`, and 100-step late-acceptance local search over list change, swap, and
reverse.

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
- `solverforge-*.dist-info/`, including metadata, the license, and the generated
  CycloneDX SBOM

The native extension embeds shared `solverforge-ui` assets and exposes them via
`solverforge.ui.asset()` and `solverforge.ui.asset_paths()` for example
applications and other Python HTTP hosts.

The source distribution contains package metadata, the README and license, the
locked Cargo manifests and Rust toolchain, and the Python/Rust sources needed to
build the package. Repository-only tests, examples, maintainer guidance,
wireframes, and tooling remain available from the source checkout. `Cargo.toml`
pins the six SolverForge crates to the exact published `0.19.3` registry base
and `solverforge-ui` to `0.7.0`; `Cargo.lock` records their crates.io checksums.

The current published Python package and native `solverforge_py` crate metadata
are version `0.6.5`, targeting the exact SolverForge `0.19.3` crate boundary.
The artifact set is one source distribution plus Linux, macOS, and Windows
wheels, verified together before release.

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
- `make pre-release`: release-base check, CI simulation, distribution build and
  validation, and a clean-venv wheel smoke test
- `make hospital-run` / `make hospital-solve`: browser or terminal hospital demo
- `make test-hospital`: hospital model, FastAPI/frontend, and Playwright tests
- `make deliveries-run` / `make deliveries-solve`: browser or terminal deliveries demo
- `make test-deliveries`: deliveries model, FastAPI/frontend, and Playwright tests

Use `make docs-check` after documentation edits so the tracked README, AGENTS,
WIREFRAME, and example README surfaces stay present and current. Dynamic move
parity closeout searches should stay clean for
`cartesian_product_move_selector is not yet bound`, unsupported dynamic selector
claims, stale `_ => false` selector fallbacks, and any top-level
`DynamicListChange` runtime type.
