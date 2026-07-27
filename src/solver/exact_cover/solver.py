"""DLX 精确覆盖求解器 — 适用于形状池、固定面积、块形约束的谜题。

核心思想：
1. 生成所有合法区域候选（形状变换 × 位置偏移）
2. 构建 DLX（舞蹈链）精确覆盖矩阵
3. Algorithm X 搜索，MRV 选列
4. 全局约束校验
"""

from __future__ import annotations

import time
from collections import defaultdict

from src.models.board import Board
from src.models.puzzle import Puzzle
from src.models.solution import Solution
from src.solver.base import Solver
from src.solver.dlx import Dlx


class ExactCoverSolver(Solver):
    name = "exact_cover"

    @classmethod
    def supports(cls, puzzle: Puzzle) -> bool:
        hw = puzzle.height * puzzle.width
        if puzzle.has_rule("shape_pool"):
            return True
        if puzzle.has_rule("block"):
            h, w = puzzle.height, puzzle.width
            est = sum((h - rh + 1) * (w - rw + 1) for rh in range(1, h + 1) for rw in range(1, w + 1))
            if puzzle.has_rule("precise") or puzzle.has_rule("range"):
                return est <= 20000
            return est <= 5000 and hw <= 80
        if puzzle.has_rule("precise"):
            return hw <= 80
        if puzzle.has_rule("range"):
            hi = puzzle.get_rule("range").params.get("max", 999)
            return hi <= 12 and hw <= 80
        if puzzle.has_rule("area"):
            max_clue = max((c.number for c in puzzle.cells if c.number is not None), default=0)
            return hw <= 80 and 2 <= max_clue <= 12
        # Estimate region size from clue count (solitary) or compass hints
        est_size = _estimate_region_size(puzzle)
        if est_size > 0 and est_size <= 12 and hw <= 80:
            return True
        return False

    def solve(self, puzzle: Puzzle, timeout: float = 30.0) -> Solution:
        self._start = time.monotonic()
        self._deadline = self._start + timeout
        self._steps = 0

        board = self._board_from_puzzle(puzzle)
        all_pos = {(r, c) for r in range(board.height) for c in range(board.width)
                   if not board.cell(r, c).blocked}
        if not all_pos:
            return Solution(board=board, solved=True, steps_taken=0, elapsed_ms=0)

        candidates = self._candidates(board, all_pos, puzzle)
        if not candidates:
            return self._fail(board, "无候选区域")

        fillable = sorted(all_pos)
        idx_map = {c: i for i, c in enumerate(fillable)}
        cell_cands: dict[tuple[int, int], list[int]] = defaultdict(list)
        for ci, cc in enumerate(candidates):
            for c in cc:
                cell_cands[c].append(ci)
        for pos in fillable:
            if not cell_cands[pos]:
                return self._fail(board, "部分单元格无候选覆盖")

        dlx = Dlx(len(fillable))
        for ci, cc in enumerate(candidates):
            cols = sorted(idx_map[c] for c in cc if c in idx_map)
            dlx.add_row(ci, cols)

        solution_rows: list[int] = []
        self._result: dict[int, set[tuple[int, int]]] | None = None

        def _cb(sol: list[int]) -> bool:
            self._steps += 1
            if time.monotonic() > self._deadline:
                return False
            # Reset board
            for r in range(board.height):
                for c in range(board.width):
                    board.cell(r, c).region_id = None
            regions: dict[int, set[tuple[int, int]]] = {}
            for ri, ci in enumerate(sol):
                regions[ri] = set(candidates[ci])
                for r, c in regions[ri]:
                    board.cell(r, c).region_id = ri
            if _global_check(board, regions, puzzle):
                self._result = regions
                return False  # found → stop
            return True

        dlx.search(solution_rows, lambda s: _cb(s))

        if self._result is None:
            return self._fail(board, "无解")

        from src.solver.propagator import update_boundary_edges
        from src.solver.validator import SolutionValidator

        update_boundary_edges(board)
        sol = SolutionValidator().validate(puzzle, board)
        sol.steps_taken = self._steps
        sol.elapsed_ms = int((time.monotonic() - self._start) * 1000)
        return sol

    def _fail(self, board: Board, msg: str) -> Solution:
        return Solution(board=board, solved=False, steps_taken=self._steps,
                        elapsed_ms=int((time.monotonic() - self._start) * 1000),
                        error_message=msg)

    def _candidates(self, board: Board, all_pos: set[tuple[int, int]],
                    puzzle: Puzzle) -> list[set[tuple[int, int]]]:
        from src.solver.shapes import all_transformations

        out: list[set[tuple[int, int]]] = []

        # ── shape_pool: 形状变换 × 位置偏移 ──
        if puzzle.has_rule("shape_pool"):
            pool = puzzle.get_rule("shape_pool")
            if pool is None:
                return out
            shapes = pool.params.get("shapes", [])
            seen: set[frozenset[tuple[int, int]]] = set()
            for seed in sorted(all_pos):
                if time.monotonic() > self._deadline:
                    break
                sr, sc = seed
                for ps in shapes:
                    for tf in all_transformations(ps.cells):
                        for rs, cs in tf:
                            dr, dc = sr - rs, sc - cs
                            placed: set[tuple[int, int]] = set()
                            ok = True
                            for r2, c2 in tf:
                                nr, nc = r2 + dr, c2 + dc
                                if (nr, nc) not in all_pos or board.cell(nr, nc).blocked:
                                    ok = False
                                    break
                                placed.add((nr, nc))
                            if not ok:
                                continue
                            fs = frozenset(placed)
                            if fs in seen:
                                continue
                            seen.add(fs)
                            if not _region_ok(board, placed, puzzle):
                                continue
                            out.append(placed)
            return out

        # ── block: 枚举所有合法矩形 ──
        if puzzle.has_rule("block"):
            h, w = puzzle.height, puzzle.width
            if puzzle.has_rule("precise"):
                lo = hi = puzzle.get_rule("precise").params["area"]
            elif puzzle.has_rule("range"):
                rr = puzzle.get_rule("range")
                lo = rr.params.get("min", 1)
                hi = rr.params.get("max", h * w)
            else:
                lo, hi = 1, h * w
            seen: set[frozenset[tuple[int, int]]] = set()
            for rh in range(1, h + 1):
                for rw in range(1, w + 1):
                    a = rh * rw
                    if a < lo or a > hi:
                        continue
                    for r in range(h - rh + 1):
                        for c in range(w - rw + 1):
                            cells = frozenset((r + dr, c + dc) for dr in range(rh) for dc in range(rw))
                            if cells in seen:
                                continue
                            seen.add(cells)
                            ss = set(cells)
                            if not ss.issubset(all_pos):
                                continue
                            if not _region_ok(board, ss, puzzle):
                                continue
                            out.append(ss)
            return out

        # ── precise small grid: BFS 枚举多联骨牌 ──
        if puzzle.has_rule("precise"):
            target = puzzle.get_rule("precise").params["area"]
            seen: set[frozenset[tuple[int, int]]] = set()
            for seed in sorted(all_pos):
                if time.monotonic() > self._deadline:
                    break
                for cand in _bfs_polyominoes(board, seed, all_pos, target, self._deadline, puzzle):
                    fs = frozenset(cand)
                    if fs not in seen:
                        seen.add(fs)
                        if _region_ok(board, cand, puzzle):
                            out.append(cand)
                        if len(out) >= 8000:
                            return out
            return out

        # ── range small grid: BFS 枚举 bounded by range ──
        if puzzle.has_rule("range"):
            rr = puzzle.get_rule("range")
            lo = rr.params.get("min", 1)
            hi = rr.params.get("max", puzzle.height * puzzle.width)
            seen: set[frozenset[tuple[int, int]]] = set()
            for seed in sorted(all_pos):
                if time.monotonic() > self._deadline:
                    break
                for target in range(lo, hi + 1):
                    for cand in _bfs_polyominoes(board, seed, all_pos, target, self._deadline, puzzle):
                        fs = frozenset(cand)
                        if fs not in seen:
                            seen.add(fs)
                            if _region_ok(board, cand, puzzle):
                                out.append(cand)
                            if len(out) >= 10000:
                                return out
            return out

        # ── estimated sizes from clues ──
        est_size = _estimate_region_size(puzzle)
        if est_size > 0 and est_size <= 12:
            lo = max(1, est_size * 3 // 4)
            hi = min(est_size * 5 // 4, puzzle.height * puzzle.width)
            seen: set[frozenset[tuple[int, int]]] = set()
            for seed in sorted(all_pos):
                if time.monotonic() > self._deadline:
                    break
                for target in range(lo, hi + 1):
                    for cand in _bfs_polyominoes(board, seed, all_pos, target, self._deadline, puzzle):
                        fs = frozenset(cand)
                        if fs not in seen:
                            seen.add(fs)
                            if _region_ok(board, cand, puzzle):
                                out.append(cand)
                            if len(out) >= 5000:
                                return out
            return out

        # ── area: BFS bounded by area clue ──
        if puzzle.has_rule("area"):
            areas = set()
            for r in range(puzzle.height):
                for c in range(puzzle.width):
                    n = board.cell(r, c).number
                    if n is not None:
                        areas.add(n)
            if not areas:
                areas = set(range(1, min(11, puzzle.height * puzzle.width + 1)))
            seen: set[frozenset[tuple[int, int]]] = set()
            for seed in sorted(all_pos):
                if time.monotonic() > self._deadline:
                    break
                for target in areas:
                    for cand in _bfs_polyominoes(board, seed, all_pos, target, self._deadline, puzzle):
                        fs = frozenset(cand)
                        if fs not in seen:
                            seen.add(fs)
                            if _region_ok(board, cand, puzzle):
                                out.append(cand)
                            if len(out) >= 10000:
                                return out
            return out

        return out

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


# ── 辅助 ──

def _region_ok(board: Board, cells: set[tuple[int, int]], puzzle: Puzzle) -> bool:
    """检查候选区域是否满足局部规则。"""
    a = len(cells)
    if puzzle.has_rule("precise") and a > puzzle.get_rule("precise").params.get("area", 0):
        return False
    if puzzle.has_rule("range"):
        rr = puzzle.get_rule("range")
        if a > rr.params.get("max", 999):
            return False
    if puzzle.has_rule("area"):
        vals = set()
        for r, c in cells:
            n = board.cell(r, c).number
            if n is not None:
                vals.add(n)
                if n < a:
                    return False
        if len(vals) > 1:
            return False
    if puzzle.has_rule("solitary"):
        clue_count = sum(1 for r, c in cells if (
            board.cell(r, c).symbol is not None or
            board.cell(r, c).compass is not None or
            board.cell(r, c).number is not None or
            board.cell(r, c).shape_pattern is not None
        ))
        if clue_count > 1:
            return False
    if puzzle.has_rule("block") and not _is_rect(cells):
        return False
    if puzzle.has_rule("non_block") and _is_rect(cells):
        return False
    # Pre-boundary check
    for r, c in cells:
        for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
            nr, nc = r + dr, c + dc
            if (nr, nc) in cells:
                pass
    return True


def _is_rect(cells: set[tuple[int, int]]) -> bool:
    if not cells:
        return False
    min_r = min(r for r, _ in cells)
    max_r = max(r for r, _ in cells)
    min_c = min(c for _, c in cells)
    max_c = max(c for _, c in cells)
    return len(cells) == (max_r - min_r + 1) * (max_c - min_c + 1)


def _bfs_polyominoes(board: Board, seed: tuple[int, int],
                     unassigned: set[tuple[int, int]], target: int,
                     deadline: float, puzzle: Puzzle) -> list[set[tuple[int, int]]]:
    out: list[set[tuple[int, int]]] = []
    seen: set[frozenset[tuple[int, int]]] = set()

    def _go(cur: set[tuple[int, int]], front: set[tuple[int, int]]):
        if time.monotonic() > deadline or len(out) >= 5000:
            return
        if len(cur) == target:
            fs = frozenset(cur)
            if fs not in seen:
                seen.add(fs)
                out.append(set(cur))
            return
        for cell in sorted(front):
            nr = cur | {cell}
            nf = (front - {cell}) | {
                nb for r, c in [cell] for nb in [(r - 1, c), (r + 1, c), (r, c - 1), (r, c + 1)]
                if nb in unassigned and nb not in nr
            }
            if not _region_ok(board, nr, puzzle):
                continue
            _go(nr, nf)

    initial = {seed}
    frontier = set()
    for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
        n = (seed[0] + dr, seed[1] + dc)
        if n in unassigned:
            frontier.add(n)
    _go(initial, frontier)
    return out


def _global_check(board: Board, regions: dict[int, set[tuple[int, int]]],
                   puzzle: Puzzle) -> bool:
    if not regions:
        return False
    from src.solver.constraints import (
        check_rule_shape_pool, check_rule_rose_window, check_rule_mixed,
        check_rule_differentiation, check_rule_brick, check_rule_ring,
        check_rule_inequality, check_rule_difference, check_rule_watchtower,
        check_rule_fence, check_rule_compass,
        check_rule_heterogeneous, check_rule_homogeneous,
        check_rule_different, check_rule_same, check_rule_block,
        check_rule_non_block, check_rule_area, check_rule_precise,
        check_rule_range, check_rule_puzzle_piece, check_rule_solitary,
        check_boundary_consistency,
    )
    for name, fn in [
        ("compass", check_rule_compass),
        ("shape_pool", check_rule_shape_pool),
        ("rose_window", check_rule_rose_window),
        ("mixed", check_rule_mixed),
        ("differentiation", check_rule_differentiation),
        ("brick", check_rule_brick),
        ("ring", check_rule_ring),
        ("inequality", check_rule_inequality),
        ("difference", check_rule_difference),
        ("watchtower", check_rule_watchtower),
        ("fence", check_rule_fence),
        ("heterogeneous", check_rule_heterogeneous),
        ("homogeneous", check_rule_homogeneous),
        ("different", check_rule_different),
        ("same", check_rule_same),
        ("block", check_rule_block),
        ("non_block", check_rule_non_block),
        ("area", check_rule_area),
        ("precise", check_rule_precise),
        ("range", check_rule_range),
        ("puzzle_piece", check_rule_puzzle_piece),
        ("solitary", check_rule_solitary),
    ]:
        if puzzle.has_rule(name) and not fn(puzzle, board):
            return False
    if hasattr(board, 'edges'):
        for e in board.edges():
            if e.is_boundary:
                pass
    return True


def _estimate_region_size(puzzle: Puzzle) -> int:
    """Estimate target region size from clues/constraints."""
    fillable = sum(1 for c in puzzle.cells if not c.blocked)
    if puzzle.has_rule("area"):
        sizes = [c.number for c in puzzle.cells if c.number is not None]
        if sizes:
            return max(sizes)
    if puzzle.has_rule("solitary"):
        clues = sum(1 for c in puzzle.cells if (
            c.symbol is not None or c.compass is not None
            or c.number is not None or c.shape_pattern is not None
        ))
        if clues > 0:
            return fillable // clues
    if puzzle.has_rule("compass"):
        max_min = 1
        for c in puzzle.cells:
            if c.compass is not None:
                s = 1 + sum(
                    max(0, getattr(c.compass, d))
                    for d in ("up", "down", "left", "right")
                )
                max_min = max(max_min, s)
        if max_min > 1:
            return max_min
    if puzzle.has_rule("rose_window"):
        from src.solver.constraints import _rose_M
        board = Board(puzzle.height, puzzle.width)
        M = _rose_M(puzzle, board)
        if M > 0:
            return fillable // M
    return 0


class FallbackExactCoverSolver(ExactCoverSolver):
    """Last-resort DLX exact cover for ANY puzzle after DFS fails."""

    name = "dlx_fallback"

    @classmethod
    def supports(cls, puzzle: Puzzle) -> bool:
        hw = puzzle.height * puzzle.width
        return hw <= 80 and _estimate_region_size(puzzle) > 0

