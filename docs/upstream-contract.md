# Upstream Contract

The binding works against public upstream crates only.

- `PlanningSolution` requires `Clone + Send + Sync + 'static`.
- `ConstraintSet<S, Score>` is the public scoring seam usable by a dynamic
  adapter.
- Upstream now exposes logical descriptor IDs and dynamic backend/slot contracts
  through `solverforge-core`, with `solverforge-bridge` re-exporting them for
  binding crates. Dynamic Python entity classes do not need distinct Rust row
  types for descriptor identity.
- Upstream now exposes `run_solver_with_config_parts`, so bindings can pass
  already-built descriptors and constraint sets instead of macro-style
  `fn() -> T` factories.
- Runtime scalar/list slots now exist as first-class `RuntimeModel` variants.
  Dynamic construction is wired through upstream construction phases for
  callback-backed scalar candidates and list element ranges. Dynamic scalar
  change/swap, nearby change/swap, pillar change/swap, ruin-recreate, grouped
  scalar, conflict repair, and compound conflict repair selectors are wired
  through upstream move selectors and score-director move application.
- Dynamic list selectors are wired through upstream list move machinery for
  list change/swap, nearby list change/swap, sublist change/swap, list reverse,
  k-opt, and list ruin. Empty dynamic default solves still stay
  construction-only.
- `limited_neighborhood`, `union_move_selector`, and two-child
  `cartesian_product_move_selector` compose supported dynamic scalar and list
  selectors.
- `SolverRuntime::detached()` exists for synchronous binding solves that should
  use the upstream runner without copying retained manager internals.
- Retained Python jobs use upstream `SolverManager<PyDynamicSolution>` and the
  same `Solvable`/runtime path as synchronous dynamic solves.
- Rust-only custom search and partitioner registration cannot be claimed as
  Python-bindable without an upstream public seam.
