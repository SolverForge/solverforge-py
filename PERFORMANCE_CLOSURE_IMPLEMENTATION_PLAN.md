# SolverForge Python performance-closure implementation plan

> **Status — completed historical plan (2026-07-18).** The wrapper runtime
> cutover described below completed in `de9355f` and is contained in the tagged
> `solverforge-py` `v0.6.1` and `v0.6.2` releases. Current package and SolverForge
> dependency versions are owned by `pyproject.toml`, `Cargo.toml`, and
> `Cargo.lock`; the future-tense steps below are retained only as the pre-cutover
> design record, not as outstanding work.
>
> Two items below are genuinely unshipped rather than retrospective, and this
> record is the only place they are written down. First, the core-owned insertion
> preview query and its Deliveries migration: no public upstream preview seam
> exists, and `examples/solverforge_deliveries/src/domain/metrics.py` still owns
> the Python `rank_delivery_insertions` candidate evaluator. Second, the source
> guard for the deleted wrapper runtime symbols listed in the validation
> sequence; the tree carries the deletion but no permanent guard against its
> return.

## Decision and scope

The Python binding must not own a second solver, construction engine, selector
tree, route evaluator, or list-candidate preview.  The one supported runtime
path is:

```text
Python declaration -> normalized immutable schema -> upstream compiled graph
-> upstream runtime execution / query -> Rust-owned dynamic state -> Python export
```

The upstream refactor is authorized by the user's subsequent instruction to
plan and implement it before refactoring the wrapper.  The benchmark source
checkout remains read-only; all comparison configuration work is confined to
`/tmp/solverforge-py-closure-bench.1WyMbB`.

The following are explicit non-goals:

- no wrapper fallback from dynamic list execution to `FirstFit`;
- no removal, emulation, or cheapest-insertion substitution of the configured
  Clarke-Wright algorithm; the existing savings/merge/completion kernel remains
  the canonical CVRP construction implementation;
- no solve-time route-to-savings inference, copied route hook, or implicit
  capability fallback; the existing flat public declaration shape, where it is
  retained for API compatibility, normalizes once at schema compilation into
  two immutable declared bundles before the core ever binds a run;
- no Python construction loop or Python score loop in benchmark solver paths;
- no public bridge callback that accepts a binding-supplied phase builder;
- no preview API that silently expands defaults, chooses an algorithm, or
  simulates a full construction run.

## Upstream SolverForge refactor

### `crates/solverforge-core/src/domain/dynamic.rs` and dynamic resolution

- Keep dynamic scalar assignment metadata declarative: it describes candidate,
  ordering, capacity, requiredness, and assignment-rule behavior but does not
  own a construction cursor.
- Split dynamic list precedence capabilities into independent duration and
  successor bits.  Resolve `Absent`, `SuccessorsOnly`, and `Explicit` once so
  ListRuin preserves successor-only behavior while scored precedence consumers
  require both values.
- Preserve dynamic list access as a concrete slot-bound adapter.  No TLS active
  slot lookup or phase-dependent metadata lookup is permitted.

### `crates/solverforge-solver/src/builder/context/{runtime_list*,list_access/*}`

- Retain one `RuntimeListSlot` carrier for typed and dynamic lists.
- Bind named immutable policies before execution: route read/replacement,
  route feasibility, savings metric class, owner, construction order, and
  precedence.  Typed historical defaults become explicit resolved policies;
  dynamic/Python requirements remain strict.
- Keep route and savings traits/bundles independent.  Clarke-Wright may read
  only savings hooks; K-opt may read only route hooks.
- Make source identity a mandatory list-slot contract without adding an
  `Eq`/`Hash` bound to native element payloads.  A static slot supplies a
  stable `usize` element-source key; dynamic `usize` elements are their own
  keys.  Per-run binding builds one core-owned compact key-to-declared-index
  resolver from the declared stream, validates duplicate/missing identities,
  and gives every generic construction kernel the same source-index bitmap.
  This is required at every public list-construction facade (Round-Robin,
  cheapest insertion, regret insertion, and Clarke-Wright): remove implicit
  `Into<usize>`/`Eq`/`Hash` recovery rather than retaining a compatibility
  constructor or an alternate keyless implementation.
  The resolver is never cached in a macro solution or a persistent graph, and
  there is no linear-scan, optional-inverse, or typed-only fallback path.

### `crates/solverforge-solver/src/runtime/compiler/{compile,construction,defaults,graph,local_search,selector_tree,slots}.rs`

- Compile one immutable graph for configured and omitted phases.
- Resolve omitted defaults per solve from frozen declaration-order bindings:
  CVRP capabilities select Clarke-Wright and then re-evaluate K-opt eligibility
  after construction, JSSP metadata selects the appropriate
  precedence/owner-aware construction, and metadata-free lists select cheapest
  insertion.  Default expansion is a staged transition, never a one-shot
  initial-state snapshot that can omit K-opt from an initially empty CW route.
  Record the exact resolved plan and policies.
- Validate every capability before instantiation.  Missing metadata is an error,
  never an execution-time fallback.
- Compile omitted local-search defaults from `RuntimeScalarSlot` and
  `RuntimeListSlot` capabilities, not from the legacy static-slot-only default
  generator.  Where the same declared capabilities exist, typed and dynamic
  models receive the same ordered selector policy; an absent capability is
  handled by the canonical default-policy rule, never by silently dropping all
  dynamic list search or choosing a wrapper substitute.
- Default phase provenance begins explicitly unresolved: the executor records
  actual staged decisions (executed, no-work, or not-reached) and performs a
  one-way terminal finalization of the candidate-trace resolved phase plan and
  digest.  Paused/intermediate detail remains incomplete; resume retains the
  same recorder.  A qualified paired result may accept only a terminal,
  complete resolved plan.
- Preserve recursive `limited`, `union`, and Cartesian laziness, selector
  order, seeded randomness, provider pull timing, move ownership, and callback
  exception behavior.
- Preserve already-public carrier-specific seeded profiles where they exist.
  In particular, legacy dynamic list-change uses a distinct dynamic salt set
  from the static list-change selector; the shared cursor receives that profile
  as data rather than silently canonicalizing it. A native-versus-Python
  bounded-work claim for a configuration that exercises such a distinct profile
  is blocked unless a complete candidate trace proves identical work. This is
  a qualification rule, never a reason to add an alternate selector path.

### Compiled execution coverage matrix

The compiler is not allowed to route a compiled variant back through a typed
or Python-specific builder. Each existing phase is first reduced to its access
protocol and then invoked by the one runtime-slot executor:

| Compiled family | Canonical implementation to retain | Compiled-executor contract |
| --- | --- | --- |
| Scalar/mixed construction | `phase/construction/engine.rs` plus descriptor/group placement policies | One frozen `RuntimeScalarSlot`/`RuntimeListSlot` kernel covers FirstFit, CheapestInsertion, decreasing/weakest/strongest, and queue forms. Its graph node carries an explicit descriptor-placement versus global-mixed scan schedule where legacy public semantics differ; the executor never rediscovers or silently unifies that choice. |
| List round-robin | `manager/phase_factory/list_construction/round_robin.rs` | Extract source ordering and commits over a prepared `RuntimeListSlot` source. |
| List cheapest insertion | `runtime/compiler/executor/list_construction/{source,cheapest,observer}.rs` | Consume the one prebound source directly. |
| List regret insertion | `manager/phase_factory/list_construction/regret.rs` | Extract owner, precedence, truncated-trial, and trace behavior over the same prepared source. |
| **Clarke-Wright** | `manager/phase_factory/list_clarke_wright/kernel.rs::run_clarke_wright` | A thin runtime phase calls the exact savings/merge/completion kernel through `RuntimeListSlot: ClarkeWrightAccess`; it is never represented as cheapest insertion. |
| List K-opt construction | `manager/phase_factory/list_k_opt.rs` | Extract route access/reconnection/feasibility/trace behavior over `RuntimeListSlot`; it has no element-source binding. |
| Grouped scalar construction | `phase/construction/grouped_scalar/phase.rs` | Preserve named assignment ownership and mandatory required completion with frozen member/placement bindings. |
| Acceptor/forager and VND | `builder/{acceptor,forager}.rs`, `phase/localsearch/*` | Reuse them with one universal runtime neighborhood carrier and normal phase termination. |
| Seven scalar selector leaves | Existing scalar selector/move algorithms | One `RuntimeScalarSlot` move/cursor carrier covers change, swap, nearby change/swap, pillar change/swap, and ruin/recreate. |
| Eleven list selector leaves | Existing list selector/move algorithms | One `RuntimeListSlot` move/cursor carrier covers change, nearby change, swap, permute, precedence, nearby swap, sublist change/swap, reverse, K-opt, and ruin. |
| Assignment selector and generic providers | Grouped-assignment stream and `runtime/provider_cursor.rs` | Lift their moves into the universal carrier without changing lazy pulls, reason-arena lifetime, salts, normalization, or tabu identity. |
| Limited/union/Cartesian | Existing decorator cursors | Compose only the universal carrier; preserve nested Cartesian and deferred right opening. |
| Typed extensions | `CustomSearchPhase` | Instantiate once per solve from the frozen typed registry; the dynamic registry is empty and rejects them structurally. |

`RuntimeListSlot` is also the single cross/intra metric boundary. A static
slot evaluates its declared intra meter with identical source/destination
entity indexes; a dynamic slot calls its own exact metadata. No macro-only or
wrapper-only metric adapter may change that behavior.

### Canonical selector migration (mandatory before routing)

The graph compiler is declarative; it is **not** permitted to grow a second
cursor/move implementation under `runtime/compiler/executor`. The existing
core selector graph is the ownership and recursive-composition authority. The
atomic migration is:

| Existing authority | Canonical end state | Removed at cutover |
| --- | --- | --- |
| `builder/selector/{build,families,types/{leaf,composite,move_union}}.rs` | One recursive runtime selector tree carrying `RuntimeScalarSlot`, `RuntimeListSlot`, grouped assignment, and provider leaves; it owns `limited`, union ordering, Cartesian preview/deferred-right behavior, candidate IDs, release, and selected ownership. The compiled graph lowers into this tree; typed macro authoring lowers into the same tree. | Separate compiler-local recursive composer and the legacy static-only `build_move_selector` assembly path. |
| `heuristic/selector/{dynamic_scalar_change,dynamic_scalar_nearby_*,list_change,list_swap,list_permute,list_reverse,list_ruin,nearby_list_*,precedence_route,sublist_*,k_opt/*}.rs` plus matching move modules | Small shared `RuntimeScalarSlot` / `RuntimeListSlot: ListAccess` leaf kernels in the established `heuristic/selector` subsystem. Each kernel owns exactly one enumeration/order/legality implementation and emits the universal runtime move carrier. Static and dynamic carriers are physical access variants only. | Dynamic-only selector/move bodies and static-only duplicate cursor bodies after their code has been extracted/migrated. |
| `runtime/compiler/executor/{scalar_neighborhood,list_neighborhood}` | Declaration-to-canonical-leaf adapters only, or deleted when their behavior lives directly in the selector subsystem. No handwritten cursor, move, emitter, precedence, or ownership algorithm may remain here. | Any copied cursor, move, emitter, or selector implementation. |

The migration is accepted only with an explicit old-to-shared-to-deleted file
map and exact parity gates for every leaf: seeded candidate order, lazy source
pull timing, `is_doable`, legal-owner/precedence filtering, candidate release,
tabu identity, selected-move ownership, apply/undo, trace labels, static versus
dynamic result state, and nested `limited`/union/Cartesian behavior. The
canonical tree owns one per-execution `ProviderReasonArena` and passes it as a
mutable context to provider leaves; it does not use `Arc`, `Rc`, mutexes, or
interior mutability in the hot path.

#### Stateful ruin/recreate contract

Ruin/recreate is not allowed to acquire a fresh random stream for each cursor
open. The canonical composed selector owns exactly one mutable
`ScalarRuinStreamState` for each frozen ruin/recreate leaf for the lifetime of
one solve/tree. Opening a cursor advances that leaf's stream while it creates
the owned batch; dropping or partially consuming the cursor preserves the
already-advanced stream exactly as today. The state borrow ends before the
cursor is returned, so no cursor uses `RefCell`, `Rc`, `Arc`, a mutex, or a
second stateful facade. The migration needs literal regressions for same-seed
sequences, successive opens, dropped cursors, partial consumption, scoped seed
isolation, union eager opening, and Cartesian deferred-right opening before the
legacy RRC cursor is removed.

#### Nearby-capability cutover decision

The historical public dynamic nearby scalar selectors silently substituted the
ordinary candidate stream or all entities when no declared nearby source was
present. That is a second candidate universe and is prohibited. At the atomic
cutover, every nearby selector (typed and dynamic) requires its declared nearby
source/capability; absence is a structural compile/bind error before any
candidate callback is pulled. The old fallthrough constructors, tests, and
wireframe wording are deleted/rewritten together. This is an intentional
upstream contract correction, with one fallible construction path rather than a
compatibility `new` plus fallback.

#### Runtime-list local-search leaf migration

The compiled graph needs a dedicated `RuntimeListMove`; `ListMoveUnion<S, V>`
cannot safely carry both typed `V` values and dynamic `usize` elements. This is
strictly local-search work and must not touch Clarke-Wright or any construction
node. The only accepted data flow is:

```text
compiled selector/default declaration -> runtime-list leaf -> shared list-kernel
coordinate cursor -> RuntimeListRecipe -> RuntimeListMove -> shared list-move
primitive -> score director
```

- `heuristic/selector/list_kernel/` is the single home for all eleven list
  coordinate streams, salts, owner snapshots, route-cycle filtering, and
  precedence analysis. Move generic ruin and precedence there, with
  `precedence/{analysis,cursor,emission}.rs` split below the source-file cap.
- `heuristic/move/list_kernel/` owns legality, mutation, undo, tabu, and trace
  primitives. Add only generic `ruin` and precedence-only `multi_swap`
  mechanics; recipes retain `RuntimeListElement::Static(V)` versus
  `RuntimeListElement::Dynamic(usize)` exactly.
- A thin compiler adapter may lower a frozen `CompiledSelectorNode::List` into
  `RuntimeListNeighborhoodPlan`, cursor, recipe, and move. It may not contain
  an enumeration, legality, callback, owner, or mutation implementation.
- First migrate and parity-test every family — change, nearby change, swap,
  permute, precedence, nearby swap, sublist change/swap, reverse, K-opt, and
  ruin — then route the recursive composer atomically. No family-specific
  interim graph route is acceptable.
- Only after exact static/dynamic parity, callback timing, ownership, undo,
  tabu, trace, default-order, and nested-composition gates pass may the old
  compiler-local `executor/list_neighborhood/` tree be deleted. Its remaining
  callers must be zero before deletion; its code must never survive as a
  second executor.

### `crates/solverforge-solver/src/runtime/compiler/executor/{prepared,execute}.rs`

- Introduce the sole compiled-graph instantiator and dispatch path.
- Route all configuration selection and slot binding through the compiled
  carriers. `CompiledRuntimeRunner` is one monomorphized `Phase`, not a
  recreated `PhaseSequence`; it instantiates once, dispatches prepared nodes
  sequentially, and finalizes the resolved trace plan only at terminal state.
  Structural instantiation may register a compact per-solve list-source
  catalog, but it must not enumerate or validate a declared source. A source
  binds exactly at the first reached source-consuming construction node, inside
  that node's normal phase-termination boundary; skipped, cancelled,
  fully-assigned, K-opt, and local-search nodes never bind it. The frozen
  declaration snapshot may be reused only within that solve (including
  pause/resume), while each kernel refreshes current assignments at use.
  Preserve existing typed extension-builder timing: configured Custom and
  Partitioned extensions instantiate during runner preparation in config
  order, after side-effect-free graph validation but before phase execution;
  source laziness must not silently change that public lifecycle contract.
  Legacy static façades delegate to extracted common kernels once those kernels
  exist. Do not rewrite, replace, or emulate Clarke-Wright with cheapest
  insertion. The duplicate path to remove is wrapper-owned phase assembly, not
  the established core construction algorithm.
- Adapt construction to normal `PhaseScope`/`StepScope` and candidate tracing;
  do not route through legacy `Construction::solve_configured` for dynamic
  lists.
- Intern callback-provider reasons per solve into compact IDs.  Candidate loops
  must not allocate/refcount `Arc` reasons or use new trait-object observers.

### `crates/solverforge-solver/src/runtime.rs`, legacy construction modules, and `run.rs`

- Replace live `build_phases` assembly atomically with compile then instantiate.
- Delete the executable legacy default expansion only after every built-in
  family runs through the compiled graph.  Do not retain a compatibility route.
- Keep typed-only Custom and Partitioned search explicitly unavailable to the
  host bridge rather than mimicking them dynamically.  Native macro models
  lower their registered typed Custom/Partitioned extensions into a
  monomorphized compiled-extension registry on that same graph; the dynamic
  bridge supplies an empty registry and receives the existing structural
  rejection.  `SearchBuilder` and generated macro `build_phases` code cannot
  remain a second assembly route for native built-ins.
- Remove or make internal every public configuration-equivalent raw phase
  factory that can assemble a second built-in construction/local-search tree.
  The generic low-level `Solver` lifecycle remains, but configured execution
  has exactly one compiler/instantiator entry point.

### `crates/solverforge-solver/src/{scope,stats,stats/candidate_trace}.rs`

- Add observed-only phase/run telemetry: phase timing, construction/local split,
  pulls/generated/evaluated/applied, score calculations, steps, first placement,
  first progress, first feasible result, timeout and overshoot.
- Use `Option` for facts that were never observed.  Emit only at phase/run
  boundaries; do not add a hot-loop instrumentation object.

### `crates/solverforge-{solver,bridge}/src/*` and public facade docs

- Expose one public compile/instantiate/runtime facade.
- Replace the public bridge runner that accepts a `BuildPhases` closure with a
  declarative compiled-runtime runner. The bridge supplies only an immutable
  `RuntimeModel`; core creates the fresh effective-config-seeded
  `SearchContext`, graph input, compiler result, and solve-owned executor.
  `runtime::compiler` remains private: neither the bridge nor a host binding
  can construct, cache, inspect, or substitute compiler graph/selector
  internals. Bindings cannot supply a second phase tree.
- Preserve the ordinary, optional-provenance, and qualified-provenance bridge
  entrypoints as thin declarations-only forwards into that one core runner.
  They differ only in immutable trace-header attestation, never in phase,
  construction, callback, or selector execution.
- Document the public capability/error contracts and update WIREFRAME files.

### One runner input, macro, and bridge cutover

- `crates/solverforge-solver/src/builder/search.rs`: replace `Search::build`,
  `build_search`, and `SearchRuntimePhase` with a declaration-only
  `RuntimeGraphInput<S, V, DM, IDM, E>` owning `SearchContext` and its concrete
  recursive extension registry.  `SearchBuilder` transfers that input without
  constructing a phase.  Keep authoring syntax (`defaults`, `phase`, and
  `partitioned_phase`) unchanged.
- `crates/solverforge-solver/src/run.rs`: replace the configured
  `BuildPhases` route used by macros and bindings with one fallible configured
  runner accepting a declaration-only factory and
  `CandidateTraceRunRequest::{Ordinary, Optional, Qualified}`. The generic
  low-level manual `Solver`/`Phase` APIs may remain only as explicitly manual
  APIs; they cannot assemble configuration-equivalent built-ins. The request
  can only affect trace-header installation. It cannot select phases or alter
  candidate work.
- `crates/solverforge-bridge/src/{runner,lib}.rs`: remove the callback closure
  from all three existing public dynamic runner variants. Each accepts only
  the immutable `RuntimeModel` declaration and thinly forwards ordinary,
  optional-provenance, or qualified-provenance intent to the one core runner.
  The bridge exposes no `RuntimeGraphInput`/compiler type. Dynamic
  custom/partitioned declarations fail structurally through the core-owned
  empty registry.
- `crates/solverforge-macros/src/planning_solution/runtime/{solve,
  helpers/phase_builders}.rs`: generate a declaration-only
  `__solverforge_runtime_declaration`, not `__solverforge_build_phases`; create
  one descriptor-sorted model and return inferred `impl Search` to the core
  configured runner, which alone calls `into_runtime_graph_input`. Stock typed
  models use `NoTypedExtensions`; typed custom/partitioned models transfer
  their concrete registry. Macro-expanded client code never has to name a
  private compiler/input type.
- The executor is concrete and monomorphized over `E::Phase`.  It owns one
  vector of built-in/extension/default stages, eagerly creates configured
  extensions once per solve in configuration order, and never uses `Box<dyn
  Phase>`, `Arc` phase erasure, a raw factory, or a bridge callback.
- Compile the graph and prepare configured typed extensions before `Solver::new`;
  source preparation is structural only at that point. Bind each list stream
  lazily at its first reached source-consuming construction stage, inside the
  existing phase-termination boundary. A direct bridge maps a reached bind or
  compile error to its existing actionable Python error boundary; retained
  solving maps it through the established `FAILED` lifecycle boundary. No
  live `Phase::solve` may manufacture an unstructured source-bind panic.
- A prepared list source freezes only the declaration payloads and the stable
  source-key index.  Every construction phase derives its *current* unassigned
  entries from that frozen index plus current assigned values.  This preserves
  repeated configured construction phases and staged defaults without a second
  declaration callback pull or stale initial-assignment snapshot.

## Core-owned insertion preview query

This follows the executor cutover; it is not allowed to land against the old
wrapper dynamic runtime.

### Upstream public contract

- Support only an **explicitly configured** `list_cheapest_insertion` target.
  Omitted defaults, regret, round-robin, Clarke-Wright, and K-opt never act as
  preview fallbacks.
- Request: exact target, declared element-stream `element_index`, and nonzero
  limit.
- Clone a private source snapshot, withdraw exactly one existing occurrence of
  the element, then evaluate the shared cheapest-placement cursor at that state.
  Zero occurrences means already unassigned; duplicates are an error.
- Evaluate all legal owner/position candidates with the normal mutation,
  notification, score, and undo protocol.  Keep only a bounded top-N heap using
  `(score descending, cursor ordinal ascending)` and clone/export detached
  solutions only after selection.
- This is a one-element placement preview, not a full construction simulation;
  other unassigned elements remain untouched.
- Query work is separately accounted.  It changes no solve telemetry, random
  state, progress, best solution, termination, candidate trace, or retained job.

### Wrapper and Deliveries migration

- `src/schema/runtime_plan.rs`/`src/schema/compiled.rs`: cache only the
  declaration-only runtime model by canonical schema provenance.  Bind the
  effective config for each solve; if a future immutable config-bound graph is
  cached, its key must include every effective config input and no mutable
  solution/callback state.
- `src/solver/api.rs`, `src/solver/solvable.rs`, `src/manager/jobs.rs`: direct,
  retained, and preview calls use the same bridge runtime.  Manager preview
  requires an exact snapshot revision and emits no event.
- Direct preview deep-copies the Python source **before** import, so shadows and
  callback views cannot mutate/attach to the caller object.  Returned candidate
  solutions are independently exported copies.
- `examples/solverforge_deliveries/src/domain/metrics.py`: remove
  `rank_delivery_insertions` and its Python candidate evaluator.  The endpoint
  maps core coordinates/scores to presentation only; it never sorts, filters,
  scores, or constructs preview routes.

## Wrapper cutover

### Keep and strengthen declarative adapters

- `src/runtime/dynamic_assignment_group.rs`: retain declarative assignment
  metadata binding; move validation to the compiled execution boundary.
- `src/runtime/{scalar_slots,list_slots}.rs`: retain direct Rust-owned state
  access and canonical metadata interpretation.  Add split precedence
  capability reporting and public successor-only ListRuin coverage.
- `src/state/{marshal,solution,callback_view}.rs`: validate every declared
  field-backed generic list metadata sequence before state creation.  Missing or
  malformed owner/order/duration/successors values are errors, never silently
  unrestricted/default metadata.  Preserve lazy callback delivery.

### Delete duplicate runtime code atomically

- Delete `src/runtime/dynamic_scalar_search.rs` only after its configuration
  facade has been superseded by the core compiled runner. This deletes no
  construction algorithm: in particular, the core `run_clarke_wright`
  savings/merge/completion implementation remains the sole Clarke-Wright path.
- Delete `src/runtime/distance.rs`, TLS/static helper modules, and old phase
  imports if unreferenced.
- Remove `PyDistanceMeter`, `PyDynamicRuntimePhase`, `build_dynamic_phases`,
  active-slot stacks, dynamic preview director, synthetic route defaults, and
  manager pre-validation that existed only for the duplicate runtime.
- Reduce `src/runtime/mod.rs` to immutable model/slot assembly.
- Add a source guard proving the deleted symbols do not remain.
- **Atomicity guard:** the existing wrapper `DynamicListClarkeWright` branch
  currently instantiates the upstream algorithm because the old upstream
  configured-construction route excludes dynamic list slots.  Only that
  wrapper *phase-assembly façade* may be deleted after the upstream compiled
  executor is live for dynamic list slots and the exact CW
  savings/merge/completion trace regression passes. `PreparedConstruction::ClarkeWright`
  must still dispatch directly to `execute_runtime_list_clarke_wright` and
  `run_clarke_wright`. There is no interim removal, synthetic
  cheapest-insertion replacement, or fallback path.

### Direct, retained, and analysis lifecycle boundary

- `src/schema/runtime_plan.rs` and `src/schema/compiled.rs`: cache the
  declaration-only compiled model, never a live phase tree.  A solve binds it
  with the complete effective `SolverConfig` into one immutable upstream run
  plan; the binding validates all selected metadata and named assignment-group
  references before execution.
- `src/solver/api.rs`: direct `solve()` imports once, builds constraints once,
  binds the same core run plan used by retained solving, executes it with
  `SolverRuntime::detached()`, refreshes terminal shadows, and exports once.
  Preserve the `catch_unwind -> panic_to_py_err` Python traceback boundary.
- `src/manager/jobs.rs`: preserve the established retained error boundary.
  The public core preflight runs synchronously only for declaration/config
  errors that already raise from `SolverManager.solve()` (notably a missing
  named assignment group); it must not turn the existing asynchronous
  `FAILED` contract for an executable invalid selector or callback into a
  synchronous exception.  In particular, do **not** compile or instantiate
  the full runtime graph during job submission.  After deepcopy/import, attach the exact immutable
  declaration/config request needed by the core executor before
  `UpstreamSolverManager::solve`.  The submitted object remains untouched; the
  working deepcopy is the only callback attachment target.
- `src/solver/solvable.rs`: bind and execute that request through the one core
  compile/instantiate path using the exact `SolverRuntime` supplied by the
  upstream manager.  It must not recreate a detached runtime, rebuild a
  wrapper phase tree, reset a seed, or discard a runtime/constraint/callback
  error. It consumes the one-shot qualified candidate-trace provenance before
  execution and selects the qualified core entrypoint exactly when present;
  ordinary and optional trace requests remain ordinary/optional rather than
  being inferred or upgraded. Runtime failures cross the existing Python-panic bridge and produce
  the one retained `FAILED` event.  Direct solve calls the same core bind and
  execute entry point synchronously; the two public error-delivery boundaries
  intentionally remain different where they already are today.
- `Solver.analyze()` remains score-only: import, evaluate constraints, refresh
  declared shadows, and export.  It must not compile/bind/instantiate a graph,
  construct, search, allocate a candidate trace, advance RNG, or invoke a
  solver callback.
- `src/state/{solution,callback_view,marshal}.rs`: candidate and snapshot
  clones must retain independent callback projection state.  Core mutations
  keep normal dirty-row synchronization; a rejected candidate or a published
  snapshot may never synchronize into the caller's attached Python object.
  `snapshot()` only clones the saved original and exports the upstream snapshot;
  it does not analyze, search, construct, or invoke callbacks.
- `src/manager/{events,jobs}.rs`: keep event/status conversion metadata-only.
  Candidate trace remains an atomic `telemetry_detail()` operation; polling,
  event draining, and snapshot serialization never create callback work or
  publish a duplicate solution.

### Python API, cache, and examples

- `python/solverforge/model.py`: cache provenance includes canonical callback
  namespace identity, not mutable global values; non-string raw schema keys are
  non-cacheable.  Preserve same-module stateless reuse.
- `python/solverforge/{solver,manager,_native.pyi,__init__}.py`: expose the
  narrowly typed preview API only after the core seam exists.  Expose
  qualified candidate-trace provenance only as an explicit immutable manager
  diagnostic value containing schema, instance, initial-state, core-tree, and
  build SHA-256 digests plus a nonempty producer.  It is never inferred from
  environment, callbacks, config, or solution state, and ordinary traces stay
  unqualified.
- `examples/solverforge_deliveries/src/data/data_seed/entrypoints.py`: seeds are
  unassigned.  Construction belongs to configured SolverForge phases.
- Preserve intentionally user-supplied initial states as public input, but never
  create one automatically in Python before solve.

## Temporary benchmark alignment

- CVRP canonical config includes the full native selector set, including both
  ruin selectors and limited sublist neighborhood; starts with empty lists.
- Employee uses native omitted phases with native seed/environment.
- JSSP uses the source-native omitted-phase profile with seed 1.
- Both sides use the same lifecycle boundary for qualified paired trials:
  retained/native and retained/Python, or direct/native and direct/Python.
  A direct Python solve is not compared with a native retained job merely
  because both report a final score.
- Native embedded TOMLs are byte-synchronized from canonical templates; Python
  adapters use the same template and overlay only whole-second duration.
- Run config provenance, fair-start, retained lifecycle, and strict candidate
  trace comparison before performance trials.
- Require a post-release-build artifact attestation before every paired run:
  deterministic current-tree hashes for wrapper and core, the exact imported
  wrapper/native extension origins and SHA-256 hashes, and an explicit build
  manifest must match at run start.  A null or stale core/build digest
  disqualifies the run rather than being treated as unknown-but-acceptable
  provenance.
- A qualified paired record also binds the terminal outcome: the trace's
  atomic retained status supplies the terminal score and snapshot revision;
  the harness fetches that exact revision, validates it, and hashes the
  benchmark's canonical planning-state projection.  `terminal_score`,
  `snapshot_revision`, `final_state_digest`, and `validator_passed` are
  required comparison keys.  A numeric cost or a separate later snapshot is
  never a substitute.  Generic core does not serialize arbitrary host
  solutions merely to provide this evidence.

## Validation sequence

1. Core policy/compiler/default-plan tests, including typed/dynamic parity.
2. Core compiled execution and candidate-trace sequence tests for CVRP, JSSP,
   Employee, recursive selectors, callbacks, and retained lifecycle.
3. Wrapper direct/retained/list/scalar/constraint/snapshot/pause/resume/cancel
   tests, including direct-versus-retained exact final values/score, original
   object isolation, independent snapshot projections, construction-time and
   local-search cancellation, pause/resume without a construction restart,
   traceback preservation, and score-only `analyze`; then examples and release
   native-extension build.
4. Source guard for deleted wrapper runtime symbols and no benchmark-source
   writes.
5. Bounded-work diagnostics with exact common config, fair start, and validators.
6. Release, CPU-pinned, warm paired trials alternating native/Python; report
   medians, dispersion, paired confidence bounds, quality, and memory/perf
   diagnostics separately.

No benchmark result is accepted merely because both solvers consumed the same
wall-clock budget.
