from __future__ import annotations

import ast
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EXAMPLES = ROOT / "examples"


def example_python_files() -> list[Path]:
    return sorted(EXAMPLES.rglob("*.py"))


def test_examples_do_not_patch_python_import_paths() -> None:
    forbidden_fragments = (
        "PYTHONPATH",
        "site.addsitedir",
        "sys.path",
        "importlib.util.spec_from_file_location",
        "importlib.machinery.SourceFileLoader",
    )

    offenders = [
        f"{path.relative_to(ROOT)} contains {fragment}"
        for path in example_python_files()
        for fragment in forbidden_fragments
        if fragment in path.read_text(encoding="utf-8")
    ]

    assert offenders == []


def test_examples_import_solverforge_as_installed_package() -> None:
    solverforge_imports: list[str] = []
    offenders: list[str] = []

    for path in example_python_files():
        relative = path.relative_to(ROOT)
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    if alias.name == "solverforge" or alias.name.startswith(
                        "solverforge."
                    ):
                        solverforge_imports.append(f"{relative}:{node.lineno}")
                    if alias.name == "python.solverforge" or alias.name.startswith(
                        "python.solverforge."
                    ):
                        offenders.append(f"{relative}:{node.lineno}")
            elif isinstance(node, ast.ImportFrom):
                module = node.module or ""
                if module == "solverforge" or module.startswith("solverforge."):
                    solverforge_imports.append(f"{relative}:{node.lineno}")
                    if node.level != 0:
                        offenders.append(f"{relative}:{node.lineno}")
                if module == "python.solverforge" or module.startswith(
                    "python.solverforge."
                ):
                    offenders.append(f"{relative}:{node.lineno}")

    assert solverforge_imports
    assert offenders == []
