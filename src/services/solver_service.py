from __future__ import annotations

import time
from typing import Optional

from src.models.puzzle import Puzzle
from src.models.solution import Solution
from src.solver.backtrack import BacktrackSolver
from src.solver.exceptions import SolverTimeoutError, NoSolutionError


class SolverService:
    def __init__(self) -> None:
        self._solver: BacktrackSolver | None = None
        self._current_solution: Solution | None = None

    def solve(self, puzzle: Puzzle, timeout: float = 30.0) -> Solution:
        self._solver = BacktrackSolver(puzzle)
        solution = self._solver.solve(timeout=timeout)
        self._current_solution = solution
        return solution

    def solve_async(self, puzzle: Puzzle, timeout: float = 30.0) -> Solution:
        return self.solve(puzzle, timeout)

    def cancel(self) -> None:
        if self._solver is not None:
            self._solver.timeout = 0

    @property
    def current_solution(self) -> Solution | None:
        return self._current_solution
