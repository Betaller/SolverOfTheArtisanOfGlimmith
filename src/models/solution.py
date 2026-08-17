from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum

from src.models.board import Board, Shape


@dataclass(slots=True)
class RegionInfo:
    region_id: int
    cells: list[tuple[int, int]]
    area: int
    shape: Shape
    normalized_shape_key: str  # canonical form hash
    matched_shape_name: str | None = None  # shape pool match


class AttemptStatus(str, Enum):
    """Terminal status of one solver-module attempt (mirrors the Rust
    `SolverStatus` enum's snake_case JSON serialization — doc 23 §3.6).

    `str, Enum` so `AttemptStatus.SUCCESS == "success"` and the value serializes
    as the bare string in JSON / UI without an explicit `.value` lookup.
    """

    SUCCESS = "success"
    TIMEOUT = "timeout"
    EXHAUSTED = "exhausted"
    VALIDATION_FAILED = "validation_failed"
    NOT_ATTEMPTED = "not_attempted"
    ERROR = "error"

    @classmethod
    def parse(cls, raw: str) -> AttemptStatus:
        """Case-insensitive parse from the Rust JSON string, falling back to
        `ERROR` for an unknown value (so a future Rust variant never crashes
        the Python consumer)."""
        norm = (raw or "").strip().lower()
        for member in cls:
            if member.value == norm:
                return member
        return cls.ERROR


@dataclass(slots=True)
class SolverAttempt:
    """One entry in a solution's per-module attempt trace (doc 23).

    Used at two levels:
      - L2 (module): `Solution.attempts` — the Rust-internal aog/rose/edge_csp/
        pieces/backtrack chain, parsed from the subprocess JSON.
      - L1 (wrapper): `SolverRouter._attempts` — the Python solver chain
        (currently just `RustSolver`).
    `elapsed_ms` is the module's own wall-clock, not cumulative.
    """

    solver: str
    status: AttemptStatus
    elapsed_ms: int = 0
    note: str | None = None
    # Legacy L1 fields kept for `SolverRouter` backward compat (steps is always
    # 0 from Rust; L1 may set it).  Default so L2 construction need not pass them.
    steps: int = 0

    @property
    def solved(self) -> bool:
        """Legacy boolean view (L1 `SolverAttempt.solved`)."""
        return self.status is AttemptStatus.SUCCESS

    @property
    def error(self) -> str | None:
        """Legacy single-string error view (L1 `SolverAttempt.error`)."""
        if self.status is AttemptStatus.SUCCESS:
            return None
        if self.note:
            return self.note
        return self.status.value


@dataclass(slots=True)
class Solution:
    board: Board | None = None
    solved: bool = False
    regions: list[RegionInfo] = field(default_factory=list)
    steps_taken: int = 0
    elapsed_ms: int = 0
    error_message: str | None = None
    rule_results: dict[str, bool] = field(default_factory=dict)
    # Which Rust solver module produced this result (aog / rose / pieces /
    # backtrack).  Empty for errors, empty-grid, or timeout placeholders.
    solver: str = ""
    # Per-module trace (L2): one entry per Rust solver module the dispatch
    # considered, in dispatch order.  Answers "which solvers ran, how long each
    # took, why each failed, which one solved it" (doc 23).  Empty for the
    # empty-grid / pre-search early returns and for L1-only (Python-router)
    # solutions that never reached a Rust subprocess.
    attempts: list[SolverAttempt] = field(default_factory=list)

    def region_of(self, r: int, c: int) -> RegionInfo | None:
        for reg in self.regions:
            if (r, c) in reg.cells:
                return reg
        return None
