# Repository Guidelines

## Project Structure & Module Organization

`solverforge-py` is a mixed Python/Rust binding package built with PyO3 and
maturin. Python package code lives under `python/solverforge/`; the native
extension, dynamic runtime bridge, callbacks, manager, and schema code live under
`src/`. Python tests are in `tests/python/`, Rust tests are in `tests/rust/`, and
examples are in `examples/`. The hospital and deliveries demos own their
FastAPI apps, app-specific static UI, generated UI models, and seed data under
`examples/solverforge_hospital/` and `examples/solverforge_deliveries/`;
shared `/sf/*` assets are served from the pinned `solverforge-ui` crate through
the native binding.
`WIREFRAME.md` is the as-built API/UI map; `README.md` carries installation,
callback/threading, boundary, and release contracts. There is no separate
`docs/` directory.

## Build, Test, and Development Commands

- `make develop`: create `.venv`, install tools, and install the release native extension.
- `make install-playwright-system-deps`: install Chromium plus Linux shared
  libraries required by Playwright browser tests in lean CI images.
- `make test`: run Rust tests with the local Python link setup, then pytest.
- `make test-quick`: run fast Python regressions without the example app tests.
- `make test-hospital`: run the hospital model, FastAPI/frontend lifecycle, and
  hospital Playwright browser test.
- `make test-deliveries`: run the deliveries model, FastAPI/frontend lifecycle,
  and deliveries Playwright browser test.
- `make test-examples-browser`: run the Playwright browser tests for both example apps.
- `make lint`: run rustfmt check, ruff, strict mypy, and clippy with warnings denied.
- `make docs-check`: verify the tracked README, AGENTS, WIREFRAME, and example
  README surface exists and avoids known stale claims.
- `make release-base-check`: verify the exact crates.io SolverForge dependency
  base in `Cargo.toml` and `Cargo.lock`.
- `make ci-local` or `make audit`: run the local CI simulation.
- `make pre-release`: run `release-base-check`, `ci-local`, release
  distribution build/checks, and clean-wheel smoke test.
- `make hospital-run`: serve the hospital app on `APP_HOST=127.0.0.1 PORT=7860`.
- `make deliveries-run`: serve the deliveries app on `APP_HOST=127.0.0.1 PORT=7860`
  unless `PORT` is overridden.

Use Rust `1.95.0` from `rust-toolchain.toml`. Python code targets Python `3.14`.

## Coding Style & Naming Conventions

Python uses Ruff linting, strict mypy, and the 100-character line limit from
`pyproject.toml`; `make py-format` is available for an intentional Black pass.
Prefer typed public APIs and keep exports in `python/solverforge/__init__.py`
intentional. Use snake_case for Python modules, functions, and variables; use
PascalCase for classes. Rust follows rustfmt and clippy with `-D warnings`; keep
module names snake_case and public types descriptive.

## Testing Guidelines

Add Python regression tests as `tests/python/test_*.py`. Add Rust integration or
unit coverage under `tests/rust/` or the relevant `src/` module. For binding
changes, prefer tests that prove Python behavior through the public `solverforge`
API, then add Rust tests only where native runtime behavior is directly changed.
Run the narrow suite first, then `make ci-local` for cross-language or
integration changes. Use `make test-hospital` or `make test-deliveries` when
touching either example app. Binding changes should normally prove behavior
through the public Python API and add Rust coverage only when native runtime
behavior changes directly.

## Runtime & Callback Contracts

Python owns model authoring: classes, decorators, functions, lambdas, and
callbacks. `solverforge-py` owns the Python API, PyO3 binding code, Rust-owned
dynamic model state, callback invocation, marshaling, packaging, and example
apps. Upstream SolverForge owns the solver engine, phases, selectors, move
machinery, termination, telemetry, retained lifecycle, descriptors, and public
bridge seams. This checkout consumes public upstream crates through exact
registry pins in `Cargo.toml` and `Cargo.lock`; do not add private upstream
module dependencies or local path overrides for release work.

One compiled schema owns one immutable runtime plan: parsed schema, descriptor,
upstream `RuntimeModel`, assignment bindings, constraints, and candidate metrics.
Direct solves, retained jobs, snapshots, clones, and resumes must use that same
plan; do not recreate wrapper-local phases, selector trees, move types, TLS slot
state, or fallback construction paths. Schema caching is allowed only for
structurally stable, capture-free callback state; stateful callbacks compile per
invocation.

Python callbacks are the only constraint authoring surface. Callback exceptions
must preserve actionable Python tracebacks. Callback solution views must expose
ordinary solution-level lookup context while projecting entity/fact collections
from Rust-owned state. Callback code may be called many times and from multiple
worker threads on CPython 3.14 free-threaded, so treat solution-level lookup
context as immutable during a solve.

`@candidate_metric` callbacks are the Python surface for named sorted or
probabilistic selector metrics. Register them through
`@planning_solution(..., candidate_metrics=[...])`; they receive the callback
solution view plus a canonical candidate identity and must return finite numeric
values. `selection_metric` is valid only with `sorted` or `probabilistic`
selection order, and probabilistic weights must be non-negative.

Ordinary dynamic scalar construction supports `first_fit` and
`cheapest_insertion`. Assignment-group construction also supports the decreasing,
weakest-fit, and strongest-fit scalar variants when their declared `entity_order`
and `value_order` capabilities satisfy the upstream compiler. Dynamic list
construction supports round-robin, cheapest/regret insertion, Clarke-Wright, and
K-opt; route-aware variants require their explicit savings or route metadata.

Candidate traces are bounded, opt-in retained diagnostics. Keep them out of
synchronous `Solver.solve`, ordinary statuses, and events; expose them only
through the atomic `SolverManager.telemetry_detail` payload. Qualified traces
require an explicit per-job `QualifiedCandidateTraceProvenance` with five
lowercase SHA-256 digests and a non-blank external producer. Never infer those
values from a solution, environment, callback, or file, and never accept
qualified provenance through serializable `SolverConfig`.

Do not fake unsupported behavior. Top-level `ConstraintFactory.join`,
`group_by`, `if_exists`, `if_not_exists`, and `flattened` remain explicit
unsupported methods until public bridge support exists for those top-level
semantics. Rust-only custom search and partitioner registration cannot be
claimed as Python-bindable without a public upstream seam.

## Release Responsibilities

`pyproject.toml` owns PyPI metadata, version, Python requirement, optional
example dependencies, project URLs, classifiers, and maturin module settings.
`Cargo.toml` owns native crate metadata and the SolverForge Rust dependency
base; the package version must match `pyproject.toml`. `Cargo.lock` locks
reproducible Rust builds. The current checkout prepares package/crate `0.6.2` on
the six SolverForge `0.19.0` registry crates and `solverforge-ui` `0.7.0`;
`make release-base-check` must stay green. The `Makefile` owns local release targets,
dependency-base checks, distribution builds, artifact validation, browser system
dependency setup, and `pre-release`. `scripts/verify_release_artifacts.py`
checks deterministic artifact metadata/content, and
`tests/python/test_release_metadata.py` guards release metadata drift.

## Commit & Pull Request Guidelines

The history uses concise Conventional Commit-style subjects such as
`fix(runtime): preserve scalar snapshots` or `test(manager): cover retained jobs`.
Pull requests should include the motivation, user-visible behavior, tests run,
and any linked issue. Include screenshots or short recordings for changes under
`examples/solverforge_hospital/static/` or
`examples/solverforge_deliveries/static/`.

## Agent-Specific Instructions

Keep changes scoped to the requested surface. Do not rewrite runtime, callback,
manager, or API payload paths for a simple documentation, UI, naming, or wiring
task unless live evidence proves that path is the failing layer. Verify assumptions
from the repo before changing critical integration code.
