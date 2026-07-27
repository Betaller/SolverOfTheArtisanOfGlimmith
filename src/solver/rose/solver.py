"""玫瑰窗专用求解器 — 包装区域匹配 + BFS 增长 + 修复算法。

适用于玫瑰窗（rose_window）谜题，特别是没有额外尺寸约束时。
按顺序尝试：region_match → rose_growth → 回退给路由器。
"""

from __future__ import annotations

import time

from src.models.board import Board
from src.models.puzzle import Puzzle
from src.models.solution import Solution
from src.solver.base import Solver
from src.solver.constraints import _rose_symbol_types, _rose_M


class RoseSolver(Solver):
    name = "rose"

    @classmethod
    def supports(cls, puzzle: Puzzle) -> bool:
        return puzzle.has_rule("rose_window")

    def solve(self, puzzle: Puzzle, timeout: float = 30.0) -> Solution:
        from src.solver.constraints import check_boundary_consistency

        start = time.monotonic()
        board = self._board_from_puzzle(puzzle)

        all_positions = {(r, c) for r in range(board.height) for c in range(board.width)
                         if not board.cell(r, c).blocked}
        if not all_positions:
            return Solution(board=board, solved=True, steps_taken=0, elapsed_ms=0)

        pre_boundaries = self._extract_pre_boundaries(board)

        # ── 尝试区域匹配求解器 ──
        from src.solver.region_match import solve_by_region_match
        result = solve_by_region_match(puzzle, board, pre_boundaries)
        if result is not None:
            if check_boundary_consistency(board):
                return self._build_solution(board, puzzle, 1, start)

        # ── 尝试 BFS 增长 + 修复 ──
        board2 = self._board_from_puzzle(puzzle)
        from src.solver.rose_growth import solve_rose_growth
        result = solve_rose_growth(puzzle, board2, pre_boundaries)
        if result is not None:
            for r in range(board.height):
                for c in range(board.width):
                    board.cell(r, c).region_id = board2.cell(r, c).region_id
            return self._build_solution(board, puzzle, 1, start)

        return Solution(board=board, solved=False, steps_taken=0,
                        elapsed_ms=int((time.monotonic() - start) * 1000),
                        error_message="玫瑰窗求解器未找到解")

    def _board_from_puzzle(self, puzzle: Puzzle) -> Board:
        board = Board(puzzle.height, puzzle.width)
        for c in puzzle.cells:
            dst = board.cell(c.row, c.col)
            dst.number = c.number
            dst.symbol = c.symbol
            dst.shape_pattern = c.shape_pattern
            dst.compass = c.compass
            dst.fence_pattern = c.fence_pattern
            dst.blocked = c.blocked
        for e in puzzle.edges:
            edge = board.edge_between(e.r1, e.c1, e.r2, e.c2)
            if edge is not None:
                edge.is_boundary = e.is_boundary
                edge.constraint = e.constraint
        for v in puzzle.vertices:
            vert = board.vertex_at(v.row, v.col)
            if vert is not None:
                vert.watchtower = v.watchtower
        board.outer_boundaries = list(puzzle.outer_boundaries)
        return board

    @staticmethod
    def _extract_pre_boundaries(board: Board) -> set[tuple[int, int, int, int]]:
        pre: set[tuple[int, int, int, int]] = set()
        for e in board.edges():
            if e.is_boundary:
                key = (e.r1, e.c1, e.r2, e.c2) if e.r1 < e.r2 or (e.r1 == e.r2 and e.c1 < e.c2) else (e.r2, e.c2, e.r1, e.c1)
                pre.add(key)
        return pre

    def _build_solution(self, board: Board, puzzle: Puzzle, steps: int, start: float) -> Solution:
        from src.solver.propagator import update_boundary_edges
        from src.solver.validator import SolutionValidator

        update_boundary_edges(board)
        sol = SolutionValidator().validate(puzzle, board)
        sol.steps_taken = steps
        sol.elapsed_ms = int((time.monotonic() - start) * 1000)
        return sol
