from __future__ import annotations

import argparse
import email
import re
import sys
import tarfile
import tomllib
import zipfile
from pathlib import Path

MIN_REPLACEMENT_VERSION = (0, 4, 0)
EXPECTED_NAME = "solverforge"
EXPECTED_REQUIRES_PYTHON = ">=3.14"
REQUIRED_URL_LABELS = {
    "Homepage",
    "Documentation",
    "Repository",
    "Bug Tracker",
    "Changelog",
}
REQUIRED_SDIST_PROJECT_PATHS = {
    "Cargo.toml",
    "Cargo.lock",
    "src/lib.rs",
    "src/bindings.rs",
}
REQUIRED_SDIST_PYTHON_PATHS = {
    "python/solverforge/__init__.py",
}
SDIST_PROJECT_ROOTS = ("", "solverforge-py/")


def fail(message: str) -> None:
    raise SystemExit(f"release artifact check failed: {message}")


def parse_version_tuple(version: str) -> tuple[int, ...]:
    if not re.fullmatch(r"\d+(?:\.\d+)*", version):
        fail(f"release version must be final numeric semver-like text, got {version!r}")
    return tuple(int(part) for part in version.split("."))


def load_toml(path: Path) -> dict[str, object]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def project_metadata(repo_root: Path) -> tuple[str, str]:
    pyproject = load_toml(repo_root / "pyproject.toml")
    cargo = load_toml(repo_root / "Cargo.toml")
    project = pyproject["project"]
    package = cargo["package"]

    if project["name"] != EXPECTED_NAME:
        fail(f"project name is {project['name']!r}, expected {EXPECTED_NAME!r}")

    project_version = str(project["version"])
    crate_version = str(package["version"])
    if project_version != crate_version:
        fail(
            f"pyproject version {project_version} does not match crate {crate_version}"
        )

    if parse_version_tuple(project_version) < MIN_REPLACEMENT_VERSION:
        fail(f"{project_version} will not replace the old PyPI architecture")

    if project["requires-python"] != EXPECTED_REQUIRES_PYTHON:
        fail(f"requires-python must be {EXPECTED_REQUIRES_PYTHON}")

    dependencies = project.get("dependencies")
    if dependencies:
        fail(
            "core runtime dependencies must stay empty; use optional dependencies for examples"
        )

    urls = project.get("urls")
    if not isinstance(urls, dict):
        fail("missing [project.urls]")
    missing_urls = REQUIRED_URL_LABELS.difference(urls)
    if missing_urls:
        fail(f"missing project URLs: {', '.join(sorted(missing_urls))}")

    return EXPECTED_NAME, project_version


def assert_wheel(path: Path, version: str) -> None:
    if not path.name.startswith(f"solverforge-{version}-"):
        fail(f"wheel {path.name} does not match version {version}")

    with zipfile.ZipFile(path) as wheel:
        names = set(wheel.namelist())
        metadata_paths = [
            name for name in names if name.endswith(".dist-info/METADATA")
        ]
        if len(metadata_paths) != 1:
            fail(f"wheel {path.name} has {len(metadata_paths)} METADATA files")

        metadata = email.message_from_bytes(wheel.read(metadata_paths[0]))
        if metadata["Name"] != EXPECTED_NAME:
            fail(f"wheel metadata name is {metadata['Name']!r}")
        if metadata["Version"] != version:
            fail(f"wheel metadata version is {metadata['Version']!r}")
        if metadata["Requires-Python"] != EXPECTED_REQUIRES_PYTHON:
            fail(f"wheel Requires-Python is {metadata['Requires-Python']!r}")

        project_urls = metadata.get_all("Project-URL", [])
        missing_urls = {
            label
            for label in REQUIRED_URL_LABELS
            if not any(item.startswith(f"{label},") for item in project_urls)
        }
        if missing_urls:
            fail(
                f"wheel metadata missing project URLs: {', '.join(sorted(missing_urls))}"
            )

        required = {
            "solverforge/__init__.py",
            "solverforge/_native.pyi",
            "solverforge/py.typed",
        }
        missing = required.difference(names)
        if missing:
            fail(f"wheel {path.name} is missing {', '.join(sorted(missing))}")

        has_native_extension = any(
            name.startswith("solverforge/_native.")
            and name.endswith((".so", ".pyd", ".dylib"))
            for name in names
        )
        if not has_native_extension:
            fail(f"wheel {path.name} does not contain the native extension")

        forbidden_prefixes = ("docs/", "examples/", "tests/", "src/")
        forbidden = sorted(
            name for name in names if name.startswith(forbidden_prefixes)
        )
        if forbidden:
            fail(f"wheel {path.name} contains source-only files such as {forbidden[0]}")

        core_requirements = [
            item
            for item in metadata.get_all("Requires-Dist", [])
            if "extra ==" not in item
        ]
        if core_requirements:
            fail(f"wheel {path.name} has core dependencies: {core_requirements}")


def assert_sdist(path: Path, version: str) -> None:
    expected_prefix = f"solverforge-{version}/"
    with tarfile.open(path, "r:gz") as sdist:
        names = set(sdist.getnames())

    required_python = {
        expected_prefix + suffix for suffix in REQUIRED_SDIST_PYTHON_PATHS
    }
    missing_python = required_python.difference(names)
    if missing_python:
        fail(f"sdist is missing {', '.join(sorted(missing_python))}")

    project_layouts = [
        {expected_prefix + root + suffix for suffix in REQUIRED_SDIST_PROJECT_PATHS}
        for root in SDIST_PROJECT_ROOTS
    ]
    if not any(layout <= names for layout in project_layouts):
        missing = min(
            (layout.difference(names) for layout in project_layouts),
            key=len,
        )
        fail(f"sdist is missing {', '.join(sorted(missing))}")

    forbidden_fragments = ("/target/", "/.venv/", "__pycache__")
    for name in names:
        if name.endswith((".so", ".pyd", ".dylib")):
            fail(f"sdist contains compiled extension {name}")
        if any(fragment in name for fragment in forbidden_fragments):
            fail(f"sdist contains generated path {name}")
        if name.startswith(f"{expected_prefix}solverforge/crates/"):
            fail(f"sdist vendors SolverForge dependency source {name}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dist", type=Path, default=Path("dist"))
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[1]
    _name, version = project_metadata(repo_root)
    dist = args.dist
    if not dist.is_dir():
        fail(f"{dist} is not a directory")

    sdists = sorted(dist.glob(f"solverforge-{version}.tar.gz"))
    if len(sdists) != 1:
        fail(f"expected one sdist for {version}, found {len(sdists)}")
    assert_sdist(sdists[0], version)

    wheels = sorted(dist.glob(f"solverforge-{version}-*.whl"))
    if not wheels:
        fail(f"expected at least one wheel for {version}")
    for wheel in wheels:
        assert_wheel(wheel, version)

    print(f"verified solverforge {version}: {len(wheels)} wheel(s), 1 sdist")
    return 0


if __name__ == "__main__":
    sys.exit(main())
