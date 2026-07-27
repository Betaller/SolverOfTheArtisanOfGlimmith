"""多求解器架构 — 接口与路由层"""

from __future__ import annotations

import time
from abc import ABC, abstractmethod
from dataclasses import dataclass

from src.models.puzzle import Puzzle
from src.models.solution import Solution


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
        router = SolverRouter([ExactCoverSolver(), BacktrackSolver()])
        solution = router.route(puzzle, timeout=30)
    """

    def __init__(self, solvers: list[Solver]) -> None:
        self._solvers = solvers
        self._attempts: list[SolverAttempt] = field(default_factory=list)

    @property
    def attempts(self) -> list[SolverAttempt]:
        return self._attempts

    def route(self, puzzle: Puzzle, timeout: float = 30.0) -> Solution:
        self._attempts = []
        start = time.monotonic()
        remaining = timeout
        pending = len(self._solvers)

        for solver in self._solvers:
            if not solver.supports(puzzle):
                continue

            budget = max(1.0, remaining / max(1, pending))
            pending -= 1
            t0 = time.monotonic()

            try:
                sol = solver.solve(puzzle, timeout=budget)
                elapsed = int((time.monotonic() - t0) * 1000)
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

            spent = time.monotonic() - start
            remaining = timeout - spent
            if remaining <= 0:
                break

        return Solution(
            solved=False,
            steps_taken=sum(a.steps for a in self._attempts),
            elapsed_ms=int((time.monotonic() - start) * 1000),
            error_message=" / ".join(
                f"[{a.solver_name}] {a.error or '无解'}" for a in self._attempts
            ),
        )


def default_router() -> SolverRouter:
    from src.solver.exact_cover.solver import ExactCoverSolver
    from src.solver.rose.solver import RoseSolver
    from src.solver.backtrack import BacktrackSolver

    return SolverRouter([
        ExactCoverSolver(),
        RoseSolver(),
        BacktrackSolver(),
    ])
