from __future__ import annotations

from collections import deque
from src.models.board import Board, Cell, Shape
from src.models.puzzle import Puzzle
from src.solver.shapes import canonical_key, match_shape_pool, is_rectangle, shapes_equal


class ConstraintPropagator:
    def __init__(self, puzzle: Puzzle) -> None:
        self.puzzle = puzzle
        self._pool_keys: set[str] | None = None
        pool_rule = puzzle.get_rule("shape_pool")
        if pool_rule is not None:
            self._pool_keys = {
                canonical_key(s.cells)
                for s in pool_rule.params.get("shapes", [])
            }

    def check_region_valid(self, board: Board, rid: int) -> bool:
        cells = board.get_region_cells(rid)
        if not cells:
            return False
        area = len(cells)

        if self.puzzle.has_rule("precise"):
            target = self.puzzle.get_rule("precise").params.get("area", 0)
            if area > target:
                return False

        range_rule = self.puzzle.get_rule("range")
        if range_rule is not None:
            min_a = range_rule.params.get("min", 0)
            max_a = range_rule.params.get("max", 999)
            if area < min_a or area > max_a:
                return False

        for c in cells:
            if c.number is not None and self.puzzle.has_rule("area"):
                if c.number < area:
                    return False

        return True

    def check_region_shape(self, board: Board, rid: int) -> bool:
        cells = board.get_region_cells(rid)
        if not cells:
            return False

        positions = [(c.row, c.col) for c in cells]
        shape = Shape(cells=frozenset(positions))
        area = len(cells)

        pool_rule = self.puzzle.get_rule("shape_pool")
        if pool_rule is not None and self._pool_keys is not None:
            nkey = canonical_key(shape.cells)
            if nkey not in self._pool_keys:
                return False

        if self.puzzle.has_rule("puzzle_piece"):
            for c in cells:
                if c.shape_pattern is not None:
                    if not shapes_equal(shape, c.shape_pattern):
                        return False

        if self.puzzle.has_rule("block"):
            if not is_rectangle(shape):
                return False

        if self.puzzle.has_rule("non_block"):
            if is_rectangle(shape):
                return False

        if self.puzzle.has_rule("precise"):
            target = self.puzzle.get_rule("precise").params.get("area", 0)
            if area != target:
                return False

        if self.puzzle.has_rule("solitary"):
            symbols = [c.symbol for c in cells if c.symbol is not None]
            if len(symbols) > 1:
                return False

        return True

    def check_region_complete(self, board: Board, rid: int) -> bool:
        cells = board.get_region_cells(rid)
        if not cells:
            return False

        for c in cells:
            if c.number is not None and self.puzzle.has_rule("area"):
                if len(cells) != c.number:
                    return False

        range_rule = self.puzzle.get_rule("range")
        if range_rule is not None:
            min_a = range_rule.params.get("min", 0)
            max_a = range_rule.params.get("max", 999)
            if not (min_a <= len(cells) <= max_a):
                return False

        if self.puzzle.has_rule("solitary"):
            symbols = [c.symbol for c in cells if c.symbol is not None]
            if len(symbols) != 1:
                return False

        return True

    def get_adjacent_region_ids(self, board: Board, rid: int) -> set[int]:
        result: set[int] = set()
        for c in board.get_region_cells(rid):
            for n in board.neighbors(c.row, c.col):
                if n.assigned and n.region_id != rid:
                    result.add(n.region_id)
        return result

    def is_region_complete(self, board: Board, rid: int) -> bool:
        for c in board.get_region_cells(rid):
            for n in board.neighbors(c.row, c.col):
                if not n.assigned and not n.blocked:
                    return False
        return True

    def find_connectivity_violations(self, board: Board) -> list[int]:
        violations: list[int] = []
        for rid in {c.region_id for c in board.cells() if c.assigned}:
            cells = board.get_region_cells(rid)
            if not cells:
                continue
            visited: set[tuple[int, int]] = set()
            stack = [(cells[0].row, cells[0].col)]
            while stack:
                r, c = stack.pop()
                if (r, c) in visited:
                    continue
                visited.add((r, c))
                for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                    nr, nc = r + dr, c + dc
                    neighbor = board.cell(nr, nc) if 0 <= nr < board.height and 0 <= nc < board.width else None
                    if neighbor is not None and neighbor.region_id == rid and (nr, nc) not in visited:
                        stack.append((nr, nc))
            if len(visited) != len(cells):
                violations.append(rid)
        return violations


def update_boundary_edges(board: Board) -> None:
    for e in board.edges():
        c1 = board.cell(e.r1, e.c1)
        c2 = board.cell(e.r2, e.c2)
        if c1.assigned and c2.assigned:
            e.is_boundary = c1.region_id != c2.region_id
