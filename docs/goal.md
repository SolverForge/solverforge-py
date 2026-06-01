# Goal: Make SolverForge Python Bindings Architecturally Real

## Objective

Implement the correct architecture for true dynamic SolverForge Python bindings.

The final result must let a Python programmer define new SolverForge models in Python without learning Rust, without generated Rust, without a string DSL, without fixed pre-modeled JSON backends, and without a duplicate 100% Python solver.

Current as-built status: the main architectural target is implemented in this
checkout. `solverforge-py` uses public SolverForge bridge crates, Rust-owned
dynamic state, callback-backed constraints, upstream dynamic runner handoff, and
upstream retained jobs. This document remains as the architectural contract and
cleanup checklist, not as evidence that the implementation is still a prototype.

The correct split is:

- Python owns model authoring: classes, decorators, functions, lambdas, callbacks.
- `solverforge-py` owns Python API, PyO3 binding code, Rust-owned dynamic model state, callback invocation, marshaling, and packaging.
- Upstream `solverforge` owns the real solver engine, built-in phases, selectors, move machinery, termination, telemetry, retained lifecycle, descriptors, and supported bridge seams.

Do not fake unsupported behavior. If a feature requires a public upstream seam, add the seam upstream cleanly and use it from `solverforge-py`.

## Repositories

Work across these sibling repositories:

- `../solverforge`
- `../solverforge-py`

Preserve unrelated dirty work. Before editing either repo, inspect:

```bash
git -C ../solverforge status --short --branch
git -C ../solverforge-py status --short --branch
```

Do not reset, revert, or overwrite unrelated user changes.

## Hard Constraints

- No generated Rust as the binding strategy.
- No expression-object constraint DSL alongside callbacks.
- No string-parsed constraints.
- No fixed pre-modeled backend as the main product.
- No second 100% Python solver.
- No duplicate implementation zoo.
- No hidden dependency on private upstream modules.
- Do not claim full support for features that remain private or typed-Rust-only upstream.
- Do not degrade the existing Rust macro/monomorphized path.
- Preserve Rust performance as much as possible by keeping solver mechanics in Rust and limiting dynamic dispatch to the foreign-language model boundary.

The Rust macro path must remain the performance ceiling. The Python path must be the highest honest dynamic binding path.

## Required Upstream Architecture

Add an official public upstream bridge layer. Prefer a new crate or clearly isolated module named one of:

- `solverforge-bridge`
- `solverforge-dynamic-runtime`

Choose the name that best matches the existing workspace style, then use it consistently.

The bridge layer must be public, documented, and usable by host-language bindings beyond Python.

It must provide:

1. Logical descriptor identity independent of Rust `TypeId`.
2. Public dynamic model access traits.
3. Public dynamic scalar/list variable slot adapters.
4. Public dynamic constraint-set support.
5. A public dynamic solver runner that accepts already-built descriptor/model/constraints/config/runtime values.
6. Runtime score support sufficient for Python score families.

### Logical Descriptor Identity

Introduce public stable IDs:

```rust
EntityClassId(usize)
VariableId(usize)
ProblemFactClassId(usize)
```

Use these IDs in the dynamic bridge path. Existing `TypeId` lookup can remain for macro-generated Rust models, but dynamic/binding models must not require one Rust entity type per user entity class.

Acceptance requirement: Python classes such as `Task`, `Shift`, and `Vehicle` can all be backed by one Rust row type without descriptor collision.

### Dynamic Model Backend

Expose a public trait equivalent to:

```rust
pub trait PlanningModelBackend: Send + Sync + Clone + 'static {
    type Score: Score;

    fn entity_count(&self, entity: EntityClassId) -> usize;

    fn get_scalar(&self, entity: EntityClassId, row: usize, var: VariableId) -> Option<usize>;
    fn set_scalar(&mut self, entity: EntityClassId, row: usize, var: VariableId, value: Option<usize>);

    fn list_len(&self, entity: EntityClassId, row: usize, var: VariableId) -> usize;
    fn list_get(&self, entity: EntityClassId, row: usize, var: VariableId, pos: usize) -> Option<usize>;
    fn list_insert(&mut self, entity: EntityClassId, row: usize, var: VariableId, pos: usize, value: usize);
    fn list_remove(&mut self, entity: EntityClassId, row: usize, var: VariableId, pos: usize) -> Option<usize>;

    fn candidate_values(&self, entity: EntityClassId, row: usize, var: VariableId) -> &[usize];
}
```

The exact API may differ if the live upstream design demands it, but it must satisfy the same contract without private modules or thread-local schema tricks.

### Dynamic Runtime Slots

Add public dynamic scalar/list slot support. Do not force host-language bindings through plain `fn` pointers that cannot carry schema/model context.

Acceptable shape:

```rust
pub enum RuntimeVariableSlot<S> {
    StaticScalar(ScalarVariableSlot<S>),
    StaticList(ListVariableSlot<S, ...>),
    DynamicScalar(Arc<dyn DynamicScalarAccess<S>>),
    DynamicList(Arc<dyn DynamicListAccess<S>>),
}
```

The exact type layout can differ, but the invariant is fixed: the macro path remains static; the bridge path gets a first-class dynamic slot path.

### Dynamic Solver Runner

Expose a public runner equivalent to:

```rust
run_dynamic_solver_with_config(
    solution,
    descriptor,
    runtime_model,
    constraints,
    config,
    runtime,
)
```

It must avoid:

- thread-local descriptor globals,
- fake `fn() -> C` adapters,
- private `ChannelProgressCallback` construction,
- copied manager lifecycle code.

It must reuse upstream solver machinery: construction heuristics, local search, selectors, move application, termination, telemetry, score director behavior, and retained lifecycle.

### Dynamic Score Handling

Solve the static `Score::levels_count()` issue correctly.

Acceptable outcomes:

- a public upstream `DynamicScore` with fixed hard/medium/soft semantics and clear score-family descriptor, or
- separate supported dynamic score types for `Soft`, `HardSoft`, and `HardMediumSoft`, or
- a true runtime score descriptor if upstream can support it coherently.

Do not pretend arbitrary bendable dimensions are supported unless the public API really supports them.

## Required `solverforge-py` Architecture

After the upstream seam exists, refactor `solverforge-py` so the Python binding uses it directly.

Required properties:

- Python decorators collect schema.
- Rust-owned dynamic tables implement the upstream dynamic backend trait.
- Python callbacks implement dynamic constraints and hooks.
- The native module calls the upstream dynamic runner.
- Synchronous solve and retained manager use the same real runtime path.
- Snapshots and best solutions are true deep clones of Rust-owned dynamic state.
- Python callback exceptions preserve actionable tracebacks.
- List variables use the upstream dynamic list slot path, not a separate Python list solver.

Do not reintroduce prototype fallback behavior that bypasses upstream solver mechanics.

Former prototype limitations now resolved:

- The private `EntityExtractor` blocker is resolved by logical descriptor IDs
  and public bridge contracts.
- The prototype scalar fallback is gone; synchronous solves use the
  upstream dynamic runner.
- Retained manager scaffolding is gone; Python jobs wrap upstream
  `SolverManager<PyDynamicSolution>`.
- Placeholder list support is gone; dynamic list slots and list local-search
  selectors are wired through the upstream move machinery.

Reintroducing any of those states is not acceptable.

## Public Python Surface

Keep one canonical Python surface:

- `planning_solution`
- `planning_entity`
- `problem_fact`
- `planning_id`
- `planning_variable`
- `planning_list_variable`
- `constraint_provider`
- `scalar_group`
- `conflict_repair`
- `ConstraintFactory`
- `Solver.solve`
- `Solver.analyze`
- `SolverManager`
- `SolverConfig`
- Python score classes

Constraints are authored with Python callbacks/functions/lambdas. Do not add a symbolic expression DSL.

The Python `ConstraintFactory` must support the planned dynamic callback stream surface:

- `for_each`
- stream-level `join`
- stream-level `filter`
- stream-level `group_by`
- stream-level `balance`
- `penalize`
- `reward`
- `named`

Top-level `join`, `group_by`, `if_exists`, `if_not_exists`, and `flattened`
remain explicit unsupported methods until public bridge support exists for those
top-level semantics.

If any method cannot be made real against the upstream bridge in this run, leave it explicit and tested as unsupported. Do not silently route it to fake semantics.

## File-Level Work Guidance

In upstream `solverforge`:

- Add the bridge crate/module and wire it into the workspace.
- Add public types for logical descriptor identity.
- Add public dynamic model backend traits.
- Add dynamic scalar/list runtime adapters.
- Add the dynamic solver runner.
- Add dynamic score support.
- Add tests proving the Rust macro path still passes unchanged.
- Add bridge tests using one Rust dynamic row type for multiple logical entity classes.
- Add list-variable bridge tests, including owner hooks and route/list operations where upstream supports them.
- Update `WIREFRAME.md` and public docs for any public upstream API additions.

In `solverforge-py`:

- Keep synchronous solves on the upstream dynamic runner.
- Implement `PyDynamicSolution` as the backend for upstream dynamic traits.
- Implement dynamic scalar/list storage and mutation through upstream bridge traits.
- Implement callback-backed constraints through the upstream dynamic constraint seam.
- Implement retained `SolverManager` over the real upstream lifecycle.
- Preserve Python API names and the one callback surface.
- Expand examples so scalar, list, mixed, manager, and traceback paths are real.
- Keep docs aligned with the real architecture and remove claims that are no longer true.

## Required Tests

Run and make pass:

```bash
cargo test --workspace --all-targets
```

from `../solverforge`, plus:

```bash
make rust-test
make py-test
make ruff
make typecheck
make docs-check
```

from `../solverforge-py`.

If the venv does not exist, create it with Python 3.14 and install the dev tools.

## Acceptance Criteria

The goal is complete only when all of these are true:

- A Python user can define and solve a new scalar model without Rust.
- A Python user can define and solve a new list-variable model without Rust.
- A Python user can define a mixed scalar/list model without Rust.
- Python callbacks are the only constraint authoring surface.
- Python solving uses upstream SolverForge solver mechanics, not a Python assignment fallback.
- The retained Python `SolverManager` is backed by the upstream retained lifecycle.
- Rust-owned dynamic state provides correct clone/snapshot behavior.
- Multiple Python entity classes can share one Rust dynamic row type without `TypeId` collision.
- Upstream Rust macro examples/tests still pass.
- The docs clearly state the performance model: Rust macro models remain fastest; dynamic Python bindings preserve the Rust solver engine but not macro-level monomorphized callback/scoring speed.

## Completion Rules

Before finishing:

- Show `git status --short --branch` for both repositories.
- Summarize every intentional upstream public API addition.
- Summarize every remaining unsupported binding feature, if any.
- Do not claim complete support for private or typed-Rust-only upstream features.
- Do not delete this file unless the user explicitly asks for cleanup.
