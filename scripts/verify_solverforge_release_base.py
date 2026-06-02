from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path
from typing import Any

SOLVERFORGE_CRATES = (
    "solverforge-bridge",
    "solverforge-config",
    "solverforge-console",
    "solverforge-core",
    "solverforge-scoring",
    "solverforge-solver",
)


class SolverForgeBase:
    def __init__(self, version: str) -> None:
        self.version = version


def fail(message: str) -> None:
    raise SystemExit(f"solverforge release base check failed: {message}")


def load_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def solverforge_base(repo_root: Path) -> SolverForgeBase:
    cargo = load_toml(repo_root / "Cargo.toml")
    package = cargo["package"]
    metadata = package.get("metadata")
    if not isinstance(metadata, dict):
        fail("Cargo.toml is missing [package.metadata.solverforge]")
    solverforge = metadata.get("solverforge")
    if not isinstance(solverforge, dict):
        fail("Cargo.toml is missing [package.metadata.solverforge]")
    version = solverforge.get("version")
    if not isinstance(version, str) or not version:
        fail("package.metadata.solverforge.version must be set")
    return SolverForgeBase(version=version)


def assert_dependency_matches(spec: object, crate: str, base: SolverForgeBase) -> None:
    if not isinstance(spec, dict):
        fail(f"{crate} must declare an exact crates.io version from package metadata")
    if "path" in spec:
        fail(f"{crate} must use crates.io, not path {spec['path']!r}")
    if "git" in spec:
        fail(f"{crate} must use crates.io, not git {spec['git']!r}")
    if "rev" in spec:
        fail(f"{crate} must use crates.io, not rev {spec['rev']!r}")
    if spec.get("version") != f"={base.version}":
        fail(f"{crate} must pin version ={base.version}")


def assert_manifest_matches(repo_root: Path, base: SolverForgeBase) -> None:
    cargo = load_toml(repo_root / "Cargo.toml")
    dependencies = cargo["dependencies"]
    for crate in SOLVERFORGE_CRATES:
        assert_dependency_matches(dependencies.get(crate), crate, base)


def assert_lock_matches(repo_root: Path, base: SolverForgeBase) -> None:
    lockfile = load_toml(repo_root / "Cargo.lock")
    packages = lockfile.get("package")
    if not isinstance(packages, list):
        fail("Cargo.lock has no package entries")
    by_name = {
        str(package.get("name")): package
        for package in packages
        if isinstance(package, dict) and str(package.get("name")) in SOLVERFORGE_CRATES
    }
    missing = sorted(set(SOLVERFORGE_CRATES) - set(by_name))
    if missing:
        fail(f"Cargo.lock is missing SolverForge crates: {', '.join(missing)}")
    for crate, package in sorted(by_name.items()):
        if package.get("version") != base.version:
            fail(
                f"{crate} lockfile version is {package.get('version')!r}, "
                f"expected {base.version!r}"
            )
        source = package.get("source")
        if not (isinstance(source, str) and source.startswith("registry+")):
            fail(f"{crate} must resolve from crates.io, got source {source!r}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--print-version", action="store_true")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[1]
    base = solverforge_base(repo_root)
    assert_manifest_matches(repo_root, base)
    if args.print_version:
        print(base.version)
        return 0
    assert_lock_matches(repo_root, base)
    print(f"SolverForge Rust crates pinned to crates.io version {base.version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
