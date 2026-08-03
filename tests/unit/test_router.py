from __future__ import annotations

import logging

from src.models.board import Board
from src.models.puzzle import Puzzle, Rule
from src.models.solution import Solution
from src.solver.base import Solver, SolverRouter


def _puzzle() -> Puzzle:
    b = Board(2, 2)
    return Puzzle(height=2, width=2, cells=b.cells(), edges=b.edges(),
                  vertices=b.vertices(), rules=[Rule.precise(4)])


def _solution_board(regions) -> Board:
    b = Board(2, 2)
    for rid, cells in enumerate(regions):
        for r, c in cells:
            b.cell(r, c).region_id = rid
    return b


class BadSolver(Solver):
    name = "bad"

    def solve(self, puzzle: Puzzle, timeout: float = 30.0) -> Solution:
        # returns a wrong answer: two separate regions violate precise(4)
        return Solution(board=_solution_board([[(0, 0), (0, 1)], [(1, 0), (1, 1)]]),
                        solved=True, regions=[])


class GoodSolver(Solver):
    name = "good"

    def solve(self, puzzle: Puzzle, timeout: float = 30.0) -> Solution:
        board = _solution_board([[(0, 0), (0, 1), (1, 0), (1, 1)]])
        return Solution(board=board, solved=True, regions=[
            __import__('src.models.solution', fromlist=['RegionInfo']).RegionInfo(
                region_id=0, cells=[(0, 0), (0, 1), (1, 0), (1, 1)], area=4,
                shape=__import__('src.models.board', fromlist=['Shape']).Shape(
                    cells=frozenset({(0, 0), (0, 1), (1, 0), (1, 1)})),
                normalized_shape_key="",
            )
        ])


class TestRouterValidationFallback:
    def test_wrong_answer_falls_through_to_next_solver(self, caplog) -> None:
        router = SolverRouter([BadSolver(), GoodSolver()])
        with caplog.at_level(logging.WARNING, logger="src.solver.base"):
            sol = router.route(_puzzle(), timeout=10, puzzle_name="test.puz")
        assert sol.solved
        assert len(sol.regions) == 1
        names = [a.solver_name for a in router.attempts]
        assert names == ["bad", "good"]
        assert router.attempts[0].solved is False
        assert router.attempts[1].solved is True
        # log recorded which solver / puzzle / error
        joined = "\n".join(r.message for r in caplog.records)
        assert "bad" in joined
        assert "test.puz" in joined
        assert "精确" in joined or "面积" in joined

    def test_all_wrong_returns_unsolved(self) -> None:
        router = SolverRouter([BadSolver()])
        sol = router.route(_puzzle(), timeout=10)
        assert not sol.solved
