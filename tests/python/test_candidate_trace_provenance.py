import copy

import pytest

from solverforge import QualifiedCandidateTraceProvenance, SolverManager, _native
from solverforge.model import _compiled_schema_for_solution
from tests.python.test_scalar_solving import Schedule

_DIGESTS = {
    "schema_sha256": "01" * 32,
    "instance_sha256": "02" * 32,
    "initial_state_sha256": "03" * 32,
    "core_tree_sha256": "04" * 32,
    "build_sha256": "05" * 32,
    "producer": "solverforge-bench",
}


class _RecordingNativeManager:
    def __init__(self) -> None:
        self.preflight_values: list[object] = []
        self.solve_values: list[tuple[object, object, object | None]] = []

    def _preflight_qualified_candidate_trace_provenance(
        self, *, qualified_candidate_trace_provenance: object | None = None
    ) -> None:
        self.preflight_values.append(qualified_candidate_trace_provenance)

    def solve(
        self,
        solution: object,
        schema: object,
        *,
        qualified_candidate_trace_provenance: object | None = None,
    ) -> int:
        self.solve_values.append(
            (solution, schema, qualified_candidate_trace_provenance)
        )
        return 17


def test_qualified_candidate_trace_provenance_constructs_exactly() -> None:
    provenance = QualifiedCandidateTraceProvenance(**_DIGESTS)

    assert provenance.schema_sha256 == _DIGESTS["schema_sha256"]
    assert provenance.instance_sha256 == _DIGESTS["instance_sha256"]
    assert provenance.initial_state_sha256 == _DIGESTS["initial_state_sha256"]
    assert provenance.core_tree_sha256 == _DIGESTS["core_tree_sha256"]
    assert provenance.build_sha256 == _DIGESTS["build_sha256"]
    assert provenance.producer == _DIGESTS["producer"]


@pytest.mark.parametrize(
    "field, value",
    [
        ("schema_sha256", "0" * 63),
        ("instance_sha256", "A" * 64),
        ("initial_state_sha256", "g" * 64),
        ("core_tree_sha256", " 0" * 32),
        ("build_sha256", "0x" + "0" * 62),
    ],
)
def test_qualified_candidate_trace_provenance_rejects_malformed_digest(
    field: str, value: str
) -> None:
    values = dict(_DIGESTS)
    values[field] = value

    with pytest.raises(ValueError, match=rf"{field}.*lowercase hexadecimal"):
        QualifiedCandidateTraceProvenance(**values)


def test_qualified_provenance_rejects_non_string_and_blank_producer() -> None:
    invalid_digest = dict(_DIGESTS)
    invalid_digest["schema_sha256"] = 1
    with pytest.raises(TypeError, match="schema_sha256 must be a str"):
        QualifiedCandidateTraceProvenance(**invalid_digest)

    blank_producer = dict(_DIGESTS)
    blank_producer["producer"] = " \t"
    with pytest.raises(ValueError, match="producer must not be empty"):
        QualifiedCandidateTraceProvenance(**blank_producer)


def test_qualified_candidate_trace_provenance_is_keyword_only() -> None:
    with pytest.raises(TypeError):
        QualifiedCandidateTraceProvenance(*_DIGESTS.values())


def test_manager_ordinary_path_skips_preflight_but_qualified_path_uses_it(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    native = _RecordingNativeManager()
    manager = SolverManager.__new__(SolverManager)
    manager._native = native  # type: ignore[assignment]
    schema = object()
    monkeypatch.setattr(
        "solverforge.manager._compiled_schema_for_solution", lambda _: schema
    )

    ordinary_solution = object()
    ordinary = manager.solve(ordinary_solution)

    assert ordinary.job_id == 17
    assert native.preflight_values == []
    assert native.solve_values == [(ordinary_solution, schema, None)]

    qualified_solution = object()
    provenance = QualifiedCandidateTraceProvenance(**_DIGESTS)
    qualified = manager.solve(
        qualified_solution,
        qualified_candidate_trace_provenance=provenance,
    )

    assert qualified.job_id == 17
    assert native.preflight_values == [provenance]
    assert native.solve_values[-1] == (qualified_solution, schema, provenance)


def test_manager_rejects_qualified_provenance_before_schema_discovery(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    manager = SolverManager()
    provenance = QualifiedCandidateTraceProvenance(**_DIGESTS)

    def unexpected_schema_discovery(_: object) -> object:
        raise AssertionError(
            "qualified provenance preflight must run before schema discovery"
        )

    monkeypatch.setattr(
        "solverforge.manager._compiled_schema_for_solution", unexpected_schema_discovery
    )

    with pytest.raises(
        RuntimeError, match="requires SolverManager configured with candidate_trace"
    ):
        manager.solve(object(), qualified_candidate_trace_provenance=provenance)


def test_native_manager_rejects_qualified_provenance_before_deepcopy(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    solution = Schedule()
    schema = _compiled_schema_for_solution(solution)
    manager = _native.SolverManager(None)
    provenance = QualifiedCandidateTraceProvenance(**_DIGESTS)

    def unexpected_deepcopy(_: object) -> object:
        raise AssertionError("qualified provenance preflight must run before deepcopy")

    monkeypatch.setattr(copy, "deepcopy", unexpected_deepcopy)

    with pytest.raises(
        RuntimeError, match="requires SolverManager configured with candidate_trace"
    ):
        manager.solve(
            solution,
            schema,
            qualified_candidate_trace_provenance=provenance,
        )


def test_manager_accepts_qualified_provenance_only_with_candidate_trace() -> None:
    manager = SolverManager({"candidate_trace": {"max_entries": 1}})
    handle = manager.solve(
        Schedule(),
        qualified_candidate_trace_provenance=QualifiedCandidateTraceProvenance(
            **_DIGESTS
        ),
    )

    assert manager.wait(handle.job_id)["lifecycle_state"] == "COMPLETED"
    manager.delete(handle.job_id)
