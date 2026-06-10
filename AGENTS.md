# Repository Guidelines

## Project Structure & Module Organization

`solverforge-py` is a mixed Python/Rust binding package built with PyO3 and
maturin. Python package code lives under `python/solverforge/`; the native
extension, dynamic runtime bridge, callbacks, manager, and schema code live under
`src/`. Python tests are in `tests/python/`, Rust tests are in `tests/rust/`, and
examples are in `examples/`. The hospital and deliveries demos own their
FastAPI apps, app-specific static UI, generated UI models, and seed data under
`examples/solverforge_hospital/` and `examples/solverforge_deliveries/`;
shared `/sf/*` assets are served from the `solverforge-ui` crate through the
native binding.
`WIREFRAME.md` is the as-built API/UI map; `docs/` contains bridge and callback
contracts.

## Build, Test, and Development Commands

- `make develop`: create `.venv`, install tools, and install the release native extension.
- `make test`: run `cargo test --locked` plus pytest.
- `make test-quick`: run fast Python regressions without the example app tests.
- `make test-hospital`: run the hospital model and FastAPI/frontend lifecycle tests.
- `make test-deliveries`: run the deliveries model and FastAPI/frontend lifecycle tests.
- `make lint`: run rustfmt check, ruff, strict mypy, and clippy with warnings denied.
- `make docs-check`: verify the tracked documentation surface exists and avoids known stale claims.
- `make ci-local` or `make audit`: run the local CI simulation.
- `make pre-release`: run `ci-local` and build the release wheel.
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

## Commit & Pull Request Guidelines

This checkout has no committed history yet, so no project-specific convention can
be inferred from Git. Use concise Conventional Commit-style subjects such as
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
