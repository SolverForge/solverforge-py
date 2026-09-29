# Entity pinning implementation plan

> **Status — implemented (2026-09-29).** Every item in §3 shipped: the
> declaration, the descriptor predicate, the per-row read, the rejection
> contract, the tests, the example, and the documentation. §5 keeps the behavior
> observed while implementing it, and §4 is the sequence that cuts the release
> from this tree. Package and dependency versions are owned by `pyproject.toml`,
> `Cargo.toml`, `Cargo.lock`, and the release tooling described in `AGENTS.md`.

Pinning preserves an entity's input planning state: its genuine scalar value and
its list ownership/positions survive construction, local search, ruin/recreate,
and exhaustive search. It is the declarative way to say "this row is input, not
decision".

## 1. Verified feasibility — no upstream or bridge change is required

The whole feature rides on public APIs already consumed by this binding. The
chain was read end to end, not inferred:

| Link | Evidence |
| --- | --- |
| Public pin API | `solverforge-core/src/domain/descriptor/entity.rs:75` `with_pin_predicate(fn(&dyn Any) -> bool)`, `:81` `is_pinned(&dyn Any, usize)`, `:90` `has_pin_predicate()`; `with_pin_field` at `:69` |
| Predicate input | `EntityDescriptor::is_pinned` calls `self.get_entity(solution, index)` (`:124`); `src/descriptor/extractor.rs:52` returns `row as &dyn Any` for `DynamicEntityRow`, so the predicate can `downcast_ref::<DynamicEntityRow>()` |
| Binding owns the descriptor | `src/schema/build.rs:12` `solution_descriptor(schema)` builds it; `src/schema/runtime_plan.rs:38` compiles it into the one immutable plan, exposed at `:51` |
| Descriptor reaches the solver | `src/solver/api.rs:60,67` and `src/solver/solvable.rs:21,30` pass `plan.descriptor().clone()` into `try_run_dynamic_solver_with_config_parts`, whose `descriptor: SolutionDescriptor` parameter is `solverforge-bridge/src/runner.rs:70` (signature at `:67`), handed to `SearchContext::try_new` at `:58` |
| Enforcement reads that descriptor | `solverforge-scoring/src/director/score_director/adapters.rs:45` `solution_descriptor()` → `solverforge-solver/src/pinning.rs` `entity_is_pinned` / `move_changes_pinned` |
| Enforcement sites (upstream, all reachable) | `heuristic/move/list_kernel/ruin.rs` (4), `manager/phase_factory/list_construction/{round_robin,kernel.rs,regret,cheapest}` , `list_clarke_wright/kernel.rs:84`, `list_k_opt/kernel.rs:94`, `k_opt.rs:256`, `phase/construction/runtime_slots/global.rs:660`, `phase/construction/forager_step.rs` (4), `phase/exhaustive/decider.rs` (3), `phase/localsearch/evaluation.rs` |

Consequence: one `.with_pin_predicate(...)` on the binding-built descriptor turns
on every enforcement site for direct, retained, snapshot, and clone solves at
once. The wrapper adds **no** enforcement code and **no** seal of its own.

## 2. Design decisions

### 2.1 Python surface: `planning_pin()` as a declared field

```python
@planning_entity
class Vehicle:
    pinned = planning_pin()
    visits = planning_list_variable(element_collection="visit_values")

    def __init__(self, vehicle_id: int, pinned: bool) -> None:
        self.vehicle_id = vehicle_id
        self.pinned = pinned          # read at import; never changed by the solve
        self.visits: list[int] = []
```

- Mirrors the upstream macro exactly: `#[planning_id]` ↔ `planning_id()`,
  `#[planning_pin]` ↔ `planning_pin()`, and the value is a plain per-instance
  bool field.
- Keeps `@planning_entity` a **bare** decorator. It is used bare in 83 places
  across the package, tests, and examples; making it accept `pin="name"` would
  force a bare/parameterized dual form plus `overload` typing under strict mypy
  for no gain.
- The pin attribute is an ordinary Python attribute. It is read once per
  instance at import, never mutated, and never exported by a solve.

### 2.2 Rejected alternatives

| Alternative | Why not |
| --- | --- |
| `@planning_entity(pin="pinned")` | Bare/parameterized decorator duality, overload typing, and 83 call sites of churn to save one declared field |
| Reusing `planning_variable(pinning=True)` | Variable-level flags cannot express entity-level semantics; upstream pins entities, not variables. The flag stays rejected, with a message pointing at `planning_pin()` |
| A Python `def is_pinned(self)` callable | `with_pin_predicate` takes a capture-free `fn(&dyn Any) -> bool`; calling Python per candidate evaluation would need per-solve global/TLS state, which the runtime contract forbids. Declared field only — a structural limit, not a fallback |
| Falling back to "not pinned" when the attribute is missing | Same class of lie as the list-metadata fallbacks this repo already forbids; a declared pin field that an instance does not carry is an error |

### 2.3 Semantics and error contract

- **Preserved state.** A pinned entity's genuine scalar keeps its input value; a
  pinned list owner keeps its current elements and positions. Unpinned entities
  in the same solve are still constructed and searched normally.
- **Required and unassigned.** A pinned *required* scalar that is unassigned at
  solve start fails the compiled mandatory-completion gate (upstream behavior:
  a pin preserves input state, it does not exempt the row from completion). A
  pinned `allows_unassigned` scalar stays unassigned. No wrapper-side
  pre-validation duplicates that gate.
- **Declaration errors**: more than one `planning_pin()` field on one entity, and
  a `planning_pin()` on a problem fact, both raise `ModelValidationError` when the
  class is decorated. A pin/variable name clash and an empty declared name are
  unreachable through the decorators, because one attribute carries exactly one
  declaration kind and attribute names are never empty; the compiled-schema
  boundary re-checks the duplicate and empty-name invariants for hand-built
  schema dictionaries.
- **Import errors** (raise at import, matching the list-metadata doctrine that
  missing or malformed declared values are errors): the attribute is unset on an
  instance — `None`, which is what an unset `planning_pin()` descriptor reads back
  as — its value is not a `bool`, or the attribute is absent entirely (reachable
  only through a hand-built schema). No case is read as "not pinned".
- **Cost.** Upstream short-circuits `move_changes_pinned` unless some descriptor
  carries a predicate, so models without pinning keep today's behavior. A model
  that declares pinning pays one descriptor scan per pinned-move check. No
  performance claim is made.

## 3. File-by-file plan

### Python package

| File | Change |
| --- | --- |
| `python/solverforge/fields.py` | Add the `planning_pin()` factory returning a `PlanningField` with `kind="planning_pin"`, alongside `planning_id()`. Reword the `planning_variable(pinning=True)` rejection to name `planning_pin()` as the supported entity-level route. |
| `python/solverforge/decorators.py` | `_collect_fields` raises `ModelValidationError` for more than one `planning_pin` field on one entity, and `problem_fact` raises for any `planning_pin` field. The pin field still rides in the entity's `fields` list carrying `kind="planning_pin"` — the shape `planning_id` already uses — so entity metadata keeps one form and the parse boundary interprets the kind. |
| `python/solverforge/model.py` | No code change: entity metadata flows through unchanged (`_infer_entity_collections` copies `__solverforge_entity__` verbatim), so the pin field reaches the schema dictionary inside `fields`. `_schema_shape` (`model.py:86`) walks dictionaries recursively, so a pin declaration already changes the compiled-schema cache shape — covered by a regression test rather than new code. |
| `python/solverforge/__init__.py` | Export `planning_pin` and add it to `__all__`. |
| `python/solverforge/_native.pyi` | No change expected: the native surface only compiles/validates a schema dict. Confirm during implementation; update only if a stub signature moves. |

### Rust binding

| File | Change |
| --- | --- |
| `src/schema/types.rs` | `EntitySchema` gains `pub pin_field: Option<String>`; `VariableSchema` is untouched (a pin is not a variable). |
| `src/schema/parse.rs` | In the entity field loop (`:37`), a field whose `kind` is `planning_pin` is lifted out of the variable list into `EntitySchema.pin_field` through `required_non_empty_str`, and a second one raises. No new entity-dictionary key is introduced. |
| `src/schema/validate.rs` | No change: both pin invariants are enforced where the field kinds are interpreted (`parse.rs`), which also covers hand-built schema dictionaries fed to `_native.compile_schema` / `_native.validate_schema`. |
| `src/schema/build.rs` | When `entity.pin_field` is set, add `.with_pin_field(intern(name))` and `.with_pin_predicate(pinned_row_predicate)` to the descriptor (`:20`), and add the module-level predicate `fn pinned_row_predicate(entity: &dyn Any) -> bool` that downcasts to `DynamicEntityRow` and reads the row flag. `intern` (`src/intern.rs`) already supplies the `&'static str`. |
| `src/state/entity_table.rs` | `DynamicEntityRow` (`:71`) gains `pub pinned: bool` (default `false`), included in `with_variable_count` and `Default`, so every clone, candidate, snapshot, and detached copy carries it. |
| `src/state/marshal.rs` | In the entity import loop (`:78`–`:122`), after the row's fields are read, `import_pin_value` resolves the declared attribute: unset (`None` → the `planning_pin()` descriptor's unset value) is "must set a bool", a non-`bool` is "must be a bool", and a truly absent attribute is its own error for hand-built schemas; otherwise `row.pinned = value`. The attribute also continues to flow into `instance_fields` as today, so callbacks can read it. |
| `src/schema/runtime_plan.rs` | No code change: the plan already owns the descriptor, and the predicate is part of it. Verify with a test that the compiled plan's descriptor reports pinned rows. |
| `src/solver/{api,solvable}.rs`, `src/manager/jobs.rs` | No change. Both entry points already forward `plan.descriptor()`, and the retained path re-imports the working deepcopy, which re-reads the pin attribute. |
| `src/state/{solution,callback_view,clone}.rs` | No change expected. Confirm a callback view and a snapshot neither mutate nor drop the flag, and that a pinned row's exported values stay the input values. |

### Tests

| File | Change |
| --- | --- |
| `tests/python/test_pinning.py` (new) | The feature matrix below. Imports `examples/pinning.py` for the end-to-end case, matching `test_list_solving.py`'s pattern. |
| `tests/python/test_decorators.py` | Only the `planning_variable(pinning=True)` assertion, extended to require that the message names `planning_pin()`. Every declaration error, the schema metadata check, the compiled-schema boundary, and the behavioral matrix live in `tests/python/test_pinning.py`, beside the models they exercise. |
| `tests/python/test_examples_import_surface.py` | No change: it walks `examples/**/*.py` generically and the new example must satisfy it. |
| `tests/rust/descriptor.rs` | Descriptor wiring: a row built with `pinned = true` reports `is_pinned` through the compiled plan's descriptor, `has_pin_predicate()` is true only when declared, and an undeclared entity reports `false`. |
| `tests/rust/runtime_slots.rs` | Fixture only: the new `pin_field` field on its `EntitySchema`. The import-path coverage lives in `tests/python/test_pinning.py`, where the public API reaches `import_solution` with real Python instances. |

Feature matrix for `tests/python/test_pinning.py`:

1. Pinned scalar keeps its input value while an unpinned control entity's scalar changes under the same config (negative control — proves pinning is what preserves it).
2. Pinned list owner keeps its element order under list construction, and under ruin/recreate; other owners still receive construction elements.
3. Pinned required scalar left unassigned fails the completion gate; pinned `allows_unassigned` scalar stays unassigned.
4. Direct and retained (`SolverManager`) solves agree on pinned final values for the same model and seed.
5. Snapshot and candidate clones neither mutate the pin attribute on the caller's instance nor lose preservation.
6. Callback views and metric/constraint callbacks observe the pinned values; scoring is unaffected by the declaration itself.
7. Declaration and import errors from §2.3, each with its actionable message.
8. Mixed model: pinned and unpinned entities of the same class in one solve.

### Examples, docs, release

| File | Change |
| --- | --- |
| `examples/pinning.py` (new) | Small runnable demo modelled on `examples/vrp_owner_hooks.py`: two vehicles, one pinned with a pre-assigned route, one free, showing the pinned route surviving the solve. |
| `README.md` | Document `planning_pin()` in the Python API list, the preserved-state and required-unassigned semantics, and the declared-field limit (no dynamic `is_pinned` callable). |
| `WIREFRAME.md` | Add the entity pin metadata to the entity surface, and record that enforcement is upstream-owned and that the binding adds no wrapper path. |
| `AGENTS.md` | One paragraph in Runtime & Callback Contracts: entity pinning is declarative, read once per instance at import, and enforced only by the compiled SolverForge runtime. (This file gates on operator approval.) |
| `CHANGELOG.md` / versions | No hand edit: `make release-tag` bumps every surface and writes the section. On this `0.x` line the `feat` computes `0.6.9`, a patch bump — only a breaking change bumps the minor here, which is the tool's decision to make, not ours. |

## 4. Commit series

1. `feat(model): pin planning entities with planning_pin()` — the whole vertical
   slice: Python surface, schema plumbing, descriptor predicate, row flag and
   import, plus `tests/python/test_pinning.py` and the Rust wiring tests. A
   commit that declared pinning without the predicate would ship a silent no-op,
   so the slice is atomic.
2. `test(pinning): cover the pinned operator matrix` — the retained/snapshot/
   callback cases from §3.4 and the negative control, if kept separate.
3. `docs(pinning): document the entity pin contract` — README, WIREFRAME,
   AGENTS.
4. `chore(release): 0.6.9` — produced by `make release-tag`, not by hand.

Alternative if item 2 is folded into item 1: keep the series at three commits
(feature, example, docs).

## 5. Risks and rollback

| Risk | Mitigation |
| --- | --- |
| A pin declaration silently becoming a no-op | One atomic feature commit wiring declaration → predicate → import; the negative-control test fails if preservation is not actually in force |
| A callback or snapshot path dropping the flag | Plain `bool` on `DynamicEntityRow` (Clone-derived), asserted by the Rust clone test and the Python snapshot test |
| Cache aliasing a pinned and an unpinned schema | `_schema_shape` walks dicts recursively; a test asserts the shape differs |
| Per-move cost in pinned models | Upstream's `has_pin_predicate` short-circuit; no perf claim, and models without pinning are unaffected |
| Scope creep into the examples' FastAPI/UI demos | The demo is a standalone script, not a change to the hospital/deliveries apps |
| Rollback | The feature is additive and inert when undeclared: revert the feature commit and every unpinned model returns to today's behavior |

Observed behavior recorded while implementing, for future readers of this plan:
a pinned scalar in a two-row solve kept its more expensive input value and ended
one soft step from optimal (`[1, 0]`, levels `[0, -10]`) while the identical
unpinned model reached `[0, 0]`, levels `[0, 0]`; a pinned list owner kept
`[0, 1]` while the free owner constructed `[2, 3]`; a pinned required scalar left
unassigned failed with `mandatory planning work incomplete`; and a pinned
`allows_unassigned` scalar stayed `None` even with a hard penalty on unassigned
rows. Two footguns are worth carrying forward: `build_schema` aliases the class's
own field list, so any caller that mutates a returned schema must copy the
containers first, and nested model classes cannot be used inside test functions
because `get_type_hints` resolves annotations at module scope.

The suite was also mutation-checked, because a pinning test can look green while
preserving nothing. With the per-row import read in `import_pin_value` disabled
and the extension rebuilt, six tests fail — pinned scalar, pinned list owner,
pinned required-unassigned, pinned optional-unassigned, retained solve, and the
unset-attribute rejection — while the two unpinned controls still pass. That is
the signature of a discriminating suite: the pin-dependent assertions fail when
the feature stops working, and the controls stay green because they describe
unpinned behavior.

## 6. Verification sequence

1. `cargo test` (Rust wiring) and `pytest tests/python/test_pinning.py tests/python/test_decorators.py`.
2. `make lint` — rustfmt, ruff, strict mypy, clippy with warnings denied.
3. `make ci-local` — the full local CI gate, including the browser tests.
4. `python examples/pinning.py` — the demo runs and shows preservation.
5. `make bump-dry` — confirm the computed version and that it bumps all nine
   version surfaces before any release is cut.
6. `make pre-release` on the tree the tag will point at, then `make release-tag`,
   push branch and tag to both remotes, and verify the published artifacts from
   PyPI (see the release reference for the endpoint caveat).
