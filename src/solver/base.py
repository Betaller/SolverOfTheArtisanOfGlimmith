"""多求解器架构 — 接口与路由层"""

from __future__ import annotations

import logging
import time
from abc import ABC, abstractmethod
from dataclasses import dataclass

from src.models.puzzle import Puzzle
from src.models.solution import Solution

logger = logging.getLogger(__name__)


class Solver(ABC):
    name: str = "base"

    @abstractmethod
    def solve(self, puzzle: Puzzle, timeout: float = 30.0) -> Solution:
        ...

    @classmethod
    def supports(cls, puzzle: Puzzle) -> bool:
        return True


@dataclass
class SolverAttempt:
    solver_name: str
    solved: bool
    elapsed_ms: int
    steps: int
    error: str | None = None


class SolverRouter:
    """按优先级链式尝试求解器，分配超时预算。

    Usage:
        router = SolverRouter([RustSolver()])
        solution = router.route(puzzle, timeout=30)
    """

    def __init__(self, solvers: list[Solver]) -> None:
        self._solvers = solvers
        self._attempts: list[SolverAttempt] = []
        self._last_verify_error: str | None = None

    @property
    def attempts(self) -> list[SolverAttempt]:
        return self._attempts

    @property
    def solvers(self) -> list[Solver]:
        return self._solvers

    def route(self, puzzle: Puzzle, timeout: float = 30.0,
              puzzle_name: str | None = None) -> Solution:
        self._attempts = []
        start = time.monotonic()

        label = puzzle_name or (
            f"{puzzle.height}x{puzzle.width} "
            f"rules={sorted(r.type for r in puzzle.rules)}"
        )

        for solver in self._solvers:
            if not solver.supports(puzzle):
                continue

            # `timeout` is a UNIT budget: every solver part (rust aog / pieces /
            # backtrack, exact_cover, rose, ...) gets the full timeout as its
            # own deadline, not a share of it.
            budget = timeout
            t0 = time.monotonic()

            try:
                sol = solver.solve(puzzle, timeout=budget)
                elapsed = int((time.monotonic() - t0) * 1000)
                if sol.solved and not self._verify_answer(solver, puzzle, sol, label):
                    # wrong answer → try the next solver
                    self._attempts.append(SolverAttempt(
                        solver_name=solver.name, solved=False,
                        elapsed_ms=elapsed, steps=sol.steps_taken,
                        error=self._last_verify_error,
                    ))
                    continue
                self._attempts.append(SolverAttempt(
                    solver_name=solver.name, solved=sol.solved,
                    elapsed_ms=elapsed, steps=sol.steps_taken,
                    error=sol.error_message,
                ))
                if sol.solved:
                    sol.elapsed_ms = int((time.monotonic() - start) * 1000)
                    return sol
            except Exception as e:
                self._attempts.append(SolverAttempt(
                    solver_name=solver.name, solved=False,
                    elapsed_ms=int((time.monotonic() - t0) * 1000),
                    steps=0, error=str(e),
                ))

        return Solution(
            solved=False,
            steps_taken=sum(a.steps for a in self._attempts),
            elapsed_ms=int((time.monotonic() - start) * 1000),
            error_message=" / ".join(
                f"[{a.solver_name}] {a.error or '无解'}" for a in self._attempts
            ),
        )

    def _verify_answer(self, solver: Solver, puzzle: Puzzle,
                       solution: Solution, label: str) -> bool:
        """Independently verify a solver's answer before accepting it.

        Uses the self-contained validator in ``src.validation`` so that a buggy
        solver (or a solver whose internal rule checks are stubs) can never
        smuggle a wrong answer through.  Logs which solver / puzzle / error.
        """
        from src.validation.validator import IndependentValidator, solution_to_board

        board = solution_to_board(puzzle, solution)
        result = IndependentValidator().validate(puzzle, board)
        if result.solved:
            return True
        self._last_verify_error = (
            "答案未通过独立验证: " + ("; ".join(result.errors[:5]) or "未知原因")
        )
        logger.warning(
            "求解器 %s 对谜题 %s 返回错误答案: %s",
            solver.name, label, "; ".join(result.errors[:5]) or "未知原因",
        )
        return False


def default_router() -> SolverRouter:
    from src.solver.rust_solver import RustSolver

    # Rust-only routing: the binary runs aog → pieces → backtrack plus the
    # rose-window solver internally.  The Python solver stack (exact_cover /
    # rose / backtrack) was removed 2026-08-06: a corpus-wide evaluation showed
    # it solved 0 puzzles the Rust stack cannot (see
    # docs/official-puzzles-status.md §C.0).  Every answer is still
    # independently re-verified via IndependentValidator in `route`.
    return SolverRouter([
        RustSolver(),
    ])
