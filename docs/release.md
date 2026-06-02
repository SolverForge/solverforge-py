# PyPI Release Plan

This repository publishes the `solverforge` package on PyPI. The first release
from this architecture is `0.4.1`.

## Current Index State

As of 2026-05-31:

- PyPI latest for `solverforge` is `0.3.0`, published on 2026-01-02.
- Published PyPI versions are `0.2.2`, `0.2.3`, `0.2.4`, `0.2.5`, `0.2.6`,
  and `0.3.0`.
- The old PyPI artifacts describe a different API and architecture, including
  `SolverFactory`, `PlanningVariable`, and Java-service requirements.
- TestPyPI has no `solverforge` project visible through its JSON API.

## Course Of Action

1. Publish a final `0.4.1`, not a prerelease. A prerelease would not become the
   default `pip install solverforge` candidate.
2. Keep `requires-python = ">=3.14"` for the new package. After `0.4.1` is
   published and smoke-tested, yank the old `0.2.x` and `0.3.0` files so Python
   3.13 users do not silently install the incompatible old architecture.
3. Build wheels and the source distribution from this repository, with the
   SolverForge Rust dependency base declared in `Cargo.toml` and locked in
   `Cargo.lock`.
4. Publish to TestPyPI first through trusted publishing.
5. Publish to PyPI from a tagged release through the reviewed `pypi`
   environment.
6. After PyPI publication, verify `python3.14 -m pip install solverforge`
   resolves to `0.4.1`.

## File Responsibilities

- `pyproject.toml`: PyPI metadata, version, Python requirement, optional example
  dependencies, project URLs, classifiers, and maturin module settings.
- `Cargo.toml`: native crate version, Rust package metadata, and the single
  SolverForge Rust dependency base. The package version must match
  `pyproject.toml`.
- `Cargo.lock`: lockfile for reproducible local and CI Rust builds.
- `Makefile`: local release targets, SolverForge dependency-base check,
  distribution build, artifact validation, and pre-release gate.
- `.github/workflows/ci.yml`: source checkout validation on pushes and pull
  requests.
- `.github/workflows/release.yml`: sdist and wheel builds, artifact validation,
  TestPyPI publishing, and PyPI publishing.
- `scripts/verify_release_artifacts.py`: deterministic metadata and artifact
  content checks.
- `tests/python/test_release_metadata.py`: regression coverage for release
  metadata that should not drift.
- `README.md`: PyPI-facing installation and architecture-transition text.
- `WIREFRAME.md`: as-built package, artifact, and example map.
- `docs/goal.md`: architecture contract without machine-local paths.
- `docs/release.md`: this release contract.

## Trusted Publishing Setup

Configure both PyPI and TestPyPI to trust:

- Owner: `SolverForge`
- Repository: `solverforge-py`
- Workflow: `release.yml`
- TestPyPI environment: `testpypi`
- PyPI environment: `pypi`

The `pypi` GitHub environment should require manual approval. The workflow uses
OIDC and does not require long-lived PyPI tokens.

## Local Gate

Run:

```bash
make pre-release
```

This checks the SolverForge dependency base from `Cargo.toml`/`Cargo.lock`, runs
local CI, builds the release source distribution and local wheel, runs
`twine check`, and verifies artifact contents.

## Publish Sequence

1. Run `make pre-release` locally.
2. Trigger the release workflow manually with `repository=testpypi`.
3. Create a clean Python 3.14 environment and install from TestPyPI.
4. Tag this repository with `v0.4.1` and push the tag.
5. Approve the `pypi` environment in GitHub Actions.
6. Verify PyPI JSON and a clean `pip install solverforge`.
7. Yank PyPI `0.2.2` through `0.3.0` with the reason:
   `Superseded by solverforge 0.4.1 dynamic Python binding architecture.`
