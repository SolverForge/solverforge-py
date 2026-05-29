class SolverForgeError(Exception):
    """Base class for SolverForge Python binding errors."""


class ModelValidationError(SolverForgeError):
    """Raised when a Python model cannot be mapped to SolverForge metadata."""


class CallbackError(SolverForgeError):
    """Raised when a user callback fails during solving or scoring."""


class NativeBridgeError(SolverForgeError):
    """Raised when the PyO3 bridge reports a native-side failure."""

