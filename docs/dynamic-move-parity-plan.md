# Dynamic Move Parity Plan

## Requirement

Python dynamic models must support every public upstream `MoveSelectorConfig`
move selector without exposing Rust macros, generating Rust, or silently
degrading one selector into another. `DynamicScalar` and `DynamicList` must both
be first-class dynamic move families, and compound scalar hooks must be Python
callbacks because upstream grouped/conflict registration is typed Rust.

## Current Selector Inventory

Scalar selectors bound for Python dynamic models:

- `change_move_selector`
- `swap_move_selector`
- `nearby_change_move_selector`
- `nearby_swap_move_selector`
- `pillar_change_move_selector`
- `pillar_swap_move_selector`
- `ruin_recreate_move_selector`
- `grouped_scalar_move_selector`
- `conflict_repair_move_selector`
- `compound_conflict_repair_move_selector`

List selectors bound for Python dynamic models:

- `list_change_move_selector`
- `nearby_list_change_move_selector`
- `list_swap_move_selector`
- `nearby_list_swap_move_selector`
- `sublist_change_move_selector`
- `sublist_swap_move_selector`
- `list_reverse_move_selector`
- `list_permute_move_selector`
- `list_precedence_move_selector`
- `k_opt_move_selector`
- `list_ruin_move_selector`

Selector combinators bound for Python dynamic models:

- `limited_neighborhood`
- `union_move_selector`
- `cartesian_product_move_selector`

## Architecture

The dynamic runtime uses one move family:

```rust
pub enum DynamicMove {
    Scalar(DynamicScalar),
    List(DynamicList),
    Cartesian(DynamicCartesianMove),
}
```

`DynamicScalar` is the scalar move union:

```rust
pub enum DynamicScalar {
    Change(...),
    Swap(...),
    PillarChange { ... },
    PillarSwap { ... },
    RuinRecreate { ... },
    Grouped(DynamicCompoundScalarMove),
    ConflictRepair(DynamicCompoundScalarMove),
    CompoundConflictRepair(DynamicCompoundScalarMove),
}
```

`DynamicList` is the list move union:

```rust
pub enum DynamicList {
    Change { ... },
    Swap { ... },
    SublistChange { ... },
    SublistSwap { ... },
    Reverse { ... },
    MultiSwap { ... },
    Permute { ... },
    KOpt { ... },
    Ruin { ... },
}
```

Nearby selectors are selector strategies, not separate move mutations. They
emit the corresponding `Change` or `Swap` variants after pruning and ordering.

Grouped scalar and conflict repair selectors use Python-declared callbacks that
return compound scalar edit candidates. Rust owns the actual solver state and
applies those edits through `DynamicScalarVariableSlot<PyDynamicSolution>`, so
clone/snapshot/move mutation semantics remain Rust-side.

## File-By-File Plan And Status

### `python/solverforge/decorators.py`

Status: implemented.

- Added `@scalar_group(name)`.
- Added `@conflict_repair(*constraint_names)`.
- Extended `@planning_solution(...)` to accept `scalar_groups=[...]` and
  `conflict_repairs=[...]`.

### `python/solverforge/model.py`

Status: implemented.

- Emits `scalar_groups` schema entries as `{name, callback}`.
- Emits `conflict_repairs` schema entries as `{constraints, callback}`.
- Validates that registered callbacks use the matching decorators.

### `python/solverforge/__init__.py`

Status: implemented.

- Exports `scalar_group`.
- Exports `conflict_repair`.

### `src/schema/mod.rs`

Status: implemented.

- `DynamicSchema` now carries Python callback registries:
  `scalar_groups: Py<PyAny>` and `conflict_repairs: Py<PyAny>`.
- `parse_schema` requires both fields from the Python model schema.

### `src/runtime/dynamic_scalar_search.rs`

Status: implemented.

- `DynamicMove` contains `Scalar`, `List`, and `Cartesian` variants.
- `DynamicScalar` contains change, swap, pillar change, pillar swap,
  ruin-recreate, grouped, conflict repair, and compound conflict repair.
- `DynamicList` contains change, swap, multi-swap, permute, sublist change,
  sublist swap, reverse, k-opt, and ruin.
- `DynamicCompoundScalarMove` applies Python callback edit candidates through
  Rust-owned dynamic scalar slots.
- Dynamic grouped scalar uses Python scalar group hooks.
- Dynamic conflict repair and compound conflict repair use Python conflict
  repair hooks and enforce configured hard-constraint validation.
- Dynamic cartesian composition supports scalar/scalar, list/list, and
  scalar/list child selectors using a preview director and composed tabu
  identity.
- `DynamicListChange` is not a top-level architecture type.

### `tests/python/test_scalar_solving.py`

Status: implemented.

- Covers scalar assignment/change.
- Covers limited neighborhood wrapping a scalar selector.
- Covers scalar swap with tabu acceptor.
- Covers pillar change.
- Covers pillar swap.
- Covers scalar ruin-recreate.
- Covers scalar/scalar cartesian composition.
- Covers grouped scalar through a Python callback.
- Covers conflict repair through a Python callback.
- Covers compound conflict repair through a Python callback.

### `tests/python/test_list_solving.py`

Status: implemented.

- Covers core list selectors through Python solves, including focused coverage
  for list permute and route/k-opt selectors.
- Covers mixed scalar/list union composition.
- Covers list/list cartesian composition.
- Covers scalar/list cartesian composition.
- Verifies list moves preserve assigned elements.

### `tests/rust/*.rs`

Status: implemented.

- Updated dynamic schema construction in Rust tests for the new callback
  registry fields.
- Existing Rust tests still cover score, clone independence, descriptors,
  runtime slots, constraint set behavior, and manager type wiring.

## Verification Commands

Required gates:

```sh
make fmt-check
make rust-test
make py-test
make typecheck
make ruff
make docs-check
```

Required searches:

```sh
rg -n "cartesian_product_move_selector is not yet bound|currently support .*dynamic|not bindable|unsupported dynamic .*selector|_ => false" src python tests examples README.md WIREFRAME.md
rg -n "DynamicListChange" src python tests examples README.md
```

Allowed final `DynamicListChange` occurrences are only references explaining
that it must not be the top-level dynamic list abstraction, or references to
upstream primitive naming. There must be no `DynamicListChange` runtime type in
`solverforge-py`.
