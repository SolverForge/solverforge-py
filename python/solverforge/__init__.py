from . import console, joiner, ui
from .config import SolverConfig, TerminationConfig
from .constraints import ConstraintFactory, indexed_presence
from .decorators import (
    conflict_repair,
    constraint_provider,
    planning_entity,
    planning_solution,
    problem_fact,
    scalar_group,
    shadow_variable_updates,
)
from .errors import (
    CallbackError,
    ModelValidationError,
    NativeBridgeError,
    SolverForgeError,
)
from .fields import planning_id, planning_list_variable, planning_variable
from .groups import (
    ScalarAssignmentGroup,
    ScalarGroupLimits,
    scalar_assignment_group,
)
from .manager import JobHandle, SolverManager
from .score import HardMediumSoftScore, HardSoftDecimalScore, HardSoftScore, SoftScore
from .solver import Solver

__version__ = "0.5.0"

__all__ = [
    "CallbackError",
    "ConstraintFactory",
    "HardMediumSoftScore",
    "HardSoftDecimalScore",
    "HardSoftScore",
    "JobHandle",
    "ModelValidationError",
    "NativeBridgeError",
    "SoftScore",
    "Solver",
    "SolverConfig",
    "SolverForgeError",
    "ScalarAssignmentGroup",
    "ScalarGroupLimits",
    "SolverManager",
    "TerminationConfig",
    "__version__",
    "console",
    "conflict_repair",
    "constraint_provider",
    "indexed_presence",
    "joiner",
    "planning_entity",
    "planning_id",
    "planning_list_variable",
    "planning_solution",
    "planning_variable",
    "problem_fact",
    "scalar_assignment_group",
    "scalar_group",
    "shadow_variable_updates",
    "ui",
]
