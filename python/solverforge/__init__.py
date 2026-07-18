from . import console, joiner, ui
from .config import SolverConfig, TerminationConfig
from .constraints import ConstraintFactory, indexed_presence
from .decorators import (
    candidate_metric,
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
from .fields import (
    CapacityRouteFeasibility,
    EntityCallback,
    ListMetadata,
    ListRouteHooks,
    ListSavingsHooks,
    RowField,
    SolutionCallback,
    SolutionField,
    planning_id,
    planning_list_variable,
    planning_variable,
)
from .groups import (
    ScalarAssignmentGroup,
    ScalarGroupLimits,
    scalar_assignment_group,
)
from ._native import QualifiedCandidateTraceProvenance
from .manager import JobHandle, SolverManager
from .score import HardMediumSoftScore, HardSoftDecimalScore, HardSoftScore, SoftScore
from .solver import Solver

__version__ = "0.6.3"

__all__ = [
    "CallbackError",
    "CapacityRouteFeasibility",
    "ConstraintFactory",
    "EntityCallback",
    "HardMediumSoftScore",
    "HardSoftDecimalScore",
    "HardSoftScore",
    "JobHandle",
    "ListMetadata",
    "ListRouteHooks",
    "ListSavingsHooks",
    "ModelValidationError",
    "NativeBridgeError",
    "QualifiedCandidateTraceProvenance",
    "SoftScore",
    "Solver",
    "SolverConfig",
    "SolverForgeError",
    "ScalarAssignmentGroup",
    "ScalarGroupLimits",
    "RowField",
    "SolverManager",
    "SolutionCallback",
    "SolutionField",
    "TerminationConfig",
    "__version__",
    "console",
    "candidate_metric",
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
