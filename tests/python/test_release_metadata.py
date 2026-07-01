from __future__ import annotations

import importlib.util
import io
import re
import tarfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def load_toml(path: Path) -> dict[str, object]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def load_release_artifact_verifier() -> object:
    path = ROOT / "scripts" / "verify_release_artifacts.py"
    spec = importlib.util.spec_from_file_location("verify_release_artifacts", path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_tgz(path: Path, names: set[str]) -> None:
    with tarfile.open(path, "w:gz") as archive:
        for name in sorted(names):
            payload = b""
            info = tarfile.TarInfo(name)
            info.size = len(payload)
            archive.addfile(info, io.BytesIO(payload))


def workflow_job(text: str, job_name: str) -> str:
    matches = list(re.finditer(r"^  [A-Za-z0-9_-]+:\s*$", text, flags=re.MULTILINE))
    for index, match in enumerate(matches):
        if match.group(0).strip() != f"{job_name}:":
            continue
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        return text[match.start() : end]
    raise AssertionError(f"workflow job {job_name!r} not found")


def test_release_version_supersedes_existing_pypi_architecture() -> None:
    import solverforge

    pyproject = load_toml(ROOT / "pyproject.toml")
    cargo = load_toml(ROOT / "Cargo.toml")

    project = pyproject["project"]
    package = cargo["package"]

    assert project["name"] == "solverforge"
    assert project["version"] == package["version"]
    assert tuple(int(part) for part in str(project["version"]).split(".")) >= (0, 4, 0)
    assert project["requires-python"] == ">=3.14"
    assert solverforge.__version__ == project["version"]


def test_core_metadata_is_runtime_only() -> None:
    pyproject = load_toml(ROOT / "pyproject.toml")
    project = pyproject["project"]

    assert project["dependencies"] == []
    optional_dependencies = project["optional-dependencies"]
    assert "examples" in optional_dependencies
    assert any(
        "fastapi" in dependency for dependency in optional_dependencies["examples"]
    )
    assert any(
        "uvicorn" in dependency for dependency in optional_dependencies["examples"]
    )


def test_project_urls_cover_release_operations() -> None:
    pyproject = load_toml(ROOT / "pyproject.toml")
    urls = pyproject["project"]["urls"]

    assert urls["Homepage"] == "https://solverforge.org"
    assert urls["Documentation"] == "https://docs.solverforge.org"
    assert urls["Repository"] == "https://github.com/SolverForge/solverforge-py"
    assert urls["Bug Tracker"] == "https://github.com/SolverForge/solverforge-py/issues"
    assert urls["Changelog"] == "https://github.com/SolverForge/solverforge-py/releases"


def test_solverforge_rust_dependency_base_is_manifest_owned() -> None:
    cargo = load_toml(ROOT / "Cargo.toml")
    solverforge = cargo["package"]["metadata"]["solverforge"]
    dependencies = cargo["dependencies"]

    assert solverforge["version"] == "0.17.1"
    assert "git" not in solverforge
    assert "rev" not in solverforge
    assert "path" not in solverforge

    for crate in (
        "solverforge-bridge",
        "solverforge-config",
        "solverforge-console",
        "solverforge-core",
        "solverforge-scoring",
        "solverforge-solver",
    ):
        spec = dependencies[crate]
        assert spec["version"] == f"={solverforge['version']}"
        assert "git" not in spec
        assert "rev" not in spec
        assert "path" not in spec


def test_solverforge_ui_dependency_is_vendored() -> None:
    cargo = load_toml(ROOT / "Cargo.toml")
    ui_spec = cargo["dependencies"]["solverforge-ui"]

    assert ui_spec == {"path": "vendor/solverforge-ui"}
    assert (ROOT / "vendor" / "solverforge-ui" / "Cargo.toml").is_file()
    assert (ROOT / "vendor" / "solverforge-ui" / "static" / "sf" / "sf.css").is_file()


def test_release_workflow_validates_only_tagged_pypi_publish() -> None:
    workflow = (ROOT / ".github" / "workflows" / "release.yml").read_text(
        encoding="utf-8"
    )
    testpypi_job = workflow_job(workflow, "publish-testpypi")
    pypi_job = workflow_job(workflow, "publish-pypi")

    assert "github.event.inputs.repository" not in workflow
    assert "PYPI_API_TOKEN" not in workflow
    assert "TEST_PYPI_API_TOKEN" not in workflow

    assert "Validate tag version" not in testpypi_job
    assert "repository-url: https://test.pypi.org/legacy/" in testpypi_job
    assert "id-token: write" in testpypi_job
    assert "password:" not in testpypi_job

    assert "Validate tag version" in pypi_job
    assert "workflow_dispatch" not in pypi_job
    assert "github.event_name == 'push'" in pypi_job
    assert "startsWith(github.ref, 'refs/tags/v')" in pypi_job
    assert "GITHUB_REF_NAME" in pypi_job
    assert "GITHUB_EVENT_NAME" not in pypi_job
    assert "pyproject.toml" in pypi_job
    assert "id-token: write" in pypi_job
    assert "password:" not in pypi_job
    assert pypi_job.index("Validate tag version") < pypi_job.index("Publish to PyPI")


def test_latest_sdist_carries_locked_project_sources_when_present() -> None:
    dists = sorted((ROOT / "dist").glob("solverforge-*.tar.gz"))
    if not dists:
        return

    latest = dists[-1]
    prefix = latest.name.removesuffix(".tar.gz")

    with tarfile.open(latest, "r:gz") as archive:
        names = set(archive.getnames())

    direct_layout = {
        f"{prefix}/Cargo.lock",
        f"{prefix}/Cargo.toml",
        f"{prefix}/src/lib.rs",
        f"{prefix}/python/solverforge/__init__.py",
    }
    nested_project_layout = {
        f"{prefix}/solverforge-py/Cargo.lock",
        f"{prefix}/solverforge-py/Cargo.toml",
        f"{prefix}/solverforge-py/src/lib.rs",
        f"{prefix}/python/solverforge/__init__.py",
    }

    assert direct_layout <= names or nested_project_layout <= names


def test_release_artifact_verifier_accepts_vendored_ui_sdist(
    tmp_path: Path,
) -> None:
    version = str(load_toml(ROOT / "pyproject.toml")["project"]["version"])
    prefix = f"solverforge-{version}"
    sdist = tmp_path / f"{prefix}.tar.gz"
    write_tgz(
        sdist,
        {
            f"{prefix}/python/solverforge/__init__.py",
            f"{prefix}/solverforge-py/Cargo.lock",
            f"{prefix}/solverforge-py/Cargo.toml",
            f"{prefix}/solverforge-py/src/bindings.rs",
            f"{prefix}/solverforge-py/src/lib.rs",
            f"{prefix}/solverforge-py/vendor/solverforge-ui/Cargo.toml",
            f"{prefix}/solverforge-py/vendor/solverforge-ui/src/assets.rs",
            f"{prefix}/solverforge-py/vendor/solverforge-ui/src/lib.rs",
            f"{prefix}/solverforge-py/vendor/solverforge-ui/static/sf/sf.css",
            f"{prefix}/solverforge-py/vendor/solverforge-ui/static/sf/sf.js",
        },
    )
    verifier = load_release_artifact_verifier()

    verifier.assert_sdist(sdist, version)
