import pytest

from solverforge import SolverConfig, TerminationConfig
from solverforge.config import _resolve_config


def test_config_toml_round_trip_shape() -> None:
    config = SolverConfig.from_toml(
        """
[termination]
seconds_spent_limit = 3
minutes_spent_limit = 1
best_score_limit = "0"
step_count_limit = 11
unimproved_step_count_limit = 7
unimproved_seconds_spent_limit = 2
"""
    )

    assert config.to_dict()["termination"] == {
        "seconds_spent_limit": 3,
        "minutes_spent_limit": 1,
        "best_score_limit": "0",
        "step_count_limit": 11,
        "unimproved_step_count_limit": 7,
        "unimproved_seconds_spent_limit": 2,
    }


def test_config_accepts_typed_termination_config() -> None:
    config = SolverConfig(
        random_seed=1,
        termination=TerminationConfig(
            best_score_limit="0hard/0soft",
            unimproved_step_count_limit=4,
        ),
    )

    assert config.to_dict() == {
        "random_seed": 1,
        "termination": {
            "best_score_limit": "0hard/0soft",
            "unimproved_step_count_limit": 4,
        },
    }


def test_config_loads_solver_toml_from_file(tmp_path) -> None:
    config_path = tmp_path / "solver.toml"
    config_path.write_text(
        "random_seed = 7\n[termination]\nseconds_spent_limit = 9\n",
        encoding="utf-8",
    )

    config = SolverConfig.load(config_path)

    assert config.to_dict()["random_seed"] == 7
    assert config.to_dict()["termination"]["seconds_spent_limit"] == 9


def test_config_load_defaults_to_solver_toml_in_current_directory(tmp_path, monkeypatch) -> None:
    config_path = tmp_path / "solver.toml"
    config_path.write_text("[termination]\nstep_count_limit = 13\n", encoding="utf-8")
    monkeypatch.chdir(tmp_path)

    config = SolverConfig.load()

    assert config.to_dict()["termination"]["step_count_limit"] == 13


def test_config_from_file_alias_loads_solver_toml(tmp_path) -> None:
    config_path = tmp_path / "solver.toml"
    config_path.write_text("[termination]\nstep_count_limit = 11\n", encoding="utf-8")

    config = SolverConfig.from_file(config_path)

    assert config.to_dict()["termination"]["step_count_limit"] == 11


def test_none_config_resolves_user_space_solver_toml(tmp_path, monkeypatch) -> None:
    config_path = tmp_path / "solver.toml"
    config_path.write_text("[termination]\nseconds_spent_limit = 17\n", encoding="utf-8")
    monkeypatch.chdir(tmp_path)

    config = _resolve_config(None)

    assert config is not None
    assert config["termination"]["seconds_spent_limit"] == 17


def test_explicit_config_overrides_user_space_solver_toml(tmp_path, monkeypatch) -> None:
    config_path = tmp_path / "solver.toml"
    config_path.write_text("[termination]\nseconds_spent_limit = 17\n", encoding="utf-8")
    monkeypatch.chdir(tmp_path)

    config = _resolve_config({"termination": {"seconds_spent_limit": 3}})

    assert config is not None
    assert config["termination"]["seconds_spent_limit"] == 3


def test_top_level_seconds_limit_dict_shortcut_normalizes_to_termination() -> None:
    config = SolverConfig.from_dict({"seconds_spent_limit": 1})

    assert config.to_dict() == {"termination": {"seconds_spent_limit": 1}}


def test_top_level_termination_shortcuts_do_not_leak_as_extra_config() -> None:
    config = SolverConfig.from_dict(
        {
            "random_seed": 7,
            "seconds_spent_limit": 1,
            "unimproved_seconds_spent_limit": 2,
        }
    )

    assert config.to_dict() == {
        "random_seed": 7,
        "termination": {
            "seconds_spent_limit": 1,
            "unimproved_seconds_spent_limit": 2,
        },
    }


def test_conflicting_top_level_and_nested_termination_fields_are_rejected() -> None:
    with pytest.raises(ValueError, match="conflicting SolverForge termination field"):
        SolverConfig.from_dict(
            {
                "seconds_spent_limit": 1,
                "termination": {"seconds_spent_limit": 2},
            }
        )


def test_config_rejects_non_upstream_termination_keys() -> None:
    with pytest.raises(ValueError, match="unknown SolverForge termination field"):
        SolverConfig.from_dict({"termination": {"move_count_limit": 13}})


def test_dict_config_rejects_non_upstream_phase_termination_keys() -> None:
    with pytest.raises(ValueError, match="unknown SolverForge termination field"):
        _resolve_config(
            {
                "phases": [
                    {
                        "type": "local_search",
                        "termination": {"move_count_limit": 13},
                    }
                ]
            }
        )


def test_phase_termination_round_trips_upstream_fields() -> None:
    config = SolverConfig.from_dict(
        {
            "phases": [
                {
                    "type": "local_search",
                    "termination": {
                        "step_count_limit": 5,
                        "unimproved_step_count_limit": 2,
                    },
                },
                {
                    "type": "partitioned_search",
                    "child_phases": [
                        {
                            "type": "local_search",
                            "termination": {"best_score_limit": "0"},
                        }
                    ],
                },
            ]
        }
    )

    assert config.to_dict()["phases"] == [
        {
            "type": "local_search",
            "termination": {
                "step_count_limit": 5,
                "unimproved_step_count_limit": 2,
            },
        },
        {
            "type": "partitioned_search",
            "child_phases": [
                {
                    "type": "local_search",
                    "termination": {"best_score_limit": "0"},
                }
            ],
        },
    ]
