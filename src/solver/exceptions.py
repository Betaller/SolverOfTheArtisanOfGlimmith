from __future__ import annotations


class SolverError(Exception):
    """Base solver exception."""


class NoSolutionError(SolverError):
    """Puzzle has no valid solution."""


class SolverTimeoutError(SolverError):
    """Solver exceeded time limit."""


class ValidationError(Exception):
    """Solution validation failed."""


class PuzzleFormatError(Exception):
    """Puzzle file format is invalid."""


class RuleConflictError(Exception):
    """Conflicting rules detected."""
