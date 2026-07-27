from __future__ import annotations

from src.models.board import Board, Cell, Shape
from src.solver.constraints import (
    check_rule_same, check_rule_different, check_rule_mixed,
    check_rule_heterogeneous, check_rule_homogeneous,
    check_rule_differentiation, check_rule_brick, check_rule_ring,
    check_rule_inequality, check_rule_difference,
    check_rule_watchtower, check_rule_fence, check_rule_compass,
    check_rule_rose_window, check_rule_shape_pool,
    check_rule_area, check_rule_precise, check_rule_range,
    check_rule_solitary, check_rule_block, check_rule_non_block,
    check_rule_puzzle_piece,
    _rose_symbol_types, _rose_M,
)
from src.solver.shapes import canonical_key


def _get_adjacent_region_ids(self, board: Board, rid: int,
                              regions: dict[int, set[tuple[int, int]]]) -> set[int]:
    result: set[int] = set()
    for r, c in regions[rid]:
        for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
            nr, nc = r + dr, c + dc
            if 0 <= nr < board.height and 0 <= nc < board.width:
                neighbor = board.cell(nr, nc)
                if neighbor.assigned and neighbor.region_id != rid:
                    result.add(neighbor.region_id)
    return result


def _check_incremental(self, board: Board, regions: dict[int, set[tuple[int, int]]],
                        new_rid: int) -> bool:
    new_cells = regions[new_rid]

    if self.puzzle.has_rule("solitary"):
        symbols = [board.cell(r, c).symbol for r, c in new_cells if board.cell(r, c).symbol is not None]
        other_clues = sum(1 for r, c in new_cells if (
            board.cell(r, c).compass is not None
            or board.cell(r, c).number is not None
            or board.cell(r, c).shape_pattern is not None
        ))
        if len(symbols) + other_clues > 1:
            return False

    if self.puzzle.has_rule("precise"):
        target = self.puzzle.get_rule("precise").params.get("area", 0)
        if len(new_cells) != target:
            return False

    if self.puzzle.has_rule("area"):
        for r, c in new_cells:
            cell = board.cell(r, c)
            if cell.number is not None and len(new_cells) < cell.number:
                return False

    if self.puzzle.has_rule("shape_pool"):
        pool_rule = self.puzzle.get_rule("shape_pool")
        if pool_rule is not None:
            pool_shapes = pool_rule.params.get("shapes", [])
            if pool_shapes and len(new_cells) not in {s.area for s in pool_shapes}:
                return False

    if self.puzzle.has_rule("rose_window"):
        rose_symbols = _rose_symbol_types(self.puzzle, board)
        if rose_symbols:
            region_syms: set[str] = set()
            for r, c in new_cells:
                sym = board.cell(r, c).symbol
                if sym is not None:
                    if sym not in rose_symbols:
                        return False
                    if sym in region_syms:
                        return False
                    region_syms.add(sym)
            if region_syms != set(rose_symbols):
                return False
            M = _rose_M(self.puzzle, board)
            if M > 0 and len(regions) > M:
                return False

    new_shape = Shape(cells=frozenset(new_cells))
    new_shape_key = canonical_key(new_shape.cells)

    if self.puzzle.has_rule("same"):
        if self._first_shape_key is None:
            self._first_shape_key = new_shape_key
        elif new_shape_key != self._first_shape_key:
            return False

    if self.puzzle.has_rule("different"):
        for rid, cells in regions.items():
            if rid == new_rid:
                continue
            other_shape = Shape(cells=frozenset(cells))
            if canonical_key(other_shape.cells) == new_shape_key:
                return False

    if self.puzzle.has_rule("mixed"):
        for other_rid in self._get_adjacent_region_ids(board, new_rid, regions):
            other_cells = regions[other_rid]
            other_shape = Shape(cells=frozenset(other_cells))
            if canonical_key(other_shape.cells) == new_shape_key:
                return False

    if self.puzzle.has_rule("heterogeneous") or self.puzzle.has_rule("homogeneous"):
        from src.models.board import EdgeConstraintType
        # Only check edges adjacent to the new region (O(new_cells) vs O(all_edges))
        checked: set[tuple] = set()
        for r, c in new_cells:
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if not (0 <= nr < board.height and 0 <= nc < board.width):
                    continue
                neighbor = board.cell(nr, nc)
                if not neighbor.assigned or neighbor.region_id == new_rid:
                    continue
                edge = board.edge_between(r, c, nr, nc)
                if edge is None or edge.constraint is None:
                    continue
                ekey = (min(r, nr), min(c, nc), max(r, nr), max(c, nc))
                if ekey in checked:
                    continue
                checked.add(ekey)
                rid1, rid2 = new_rid, neighbor.region_id
                if rid1 not in regions or rid2 not in regions:
                    continue
                cells1 = regions[rid1]
                cells2 = regions[rid2]
                s1 = Shape(cells=frozenset(cells1))
                s2 = Shape(cells=frozenset(cells2))
                eq = canonical_key(s1.cells) == canonical_key(s2.cells)
                if edge.constraint.type == EdgeConstraintType.HETEROGENEOUS and eq:
                    return False
                if edge.constraint.type == EdgeConstraintType.HOMOGENEOUS and not eq:
                    return False

    if self.puzzle.has_rule("differentiation"):
        for other_rid in self._get_adjacent_region_ids(board, new_rid, regions):
            if len(regions[other_rid]) == len(new_cells):
                return False

    if self._pre_boundaries:
        for r, c in new_cells:
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if 0 <= nr < board.height and 0 <= nc < board.width:
                    neighbor = board.cell(nr, nc)
                    if neighbor.assigned and neighbor.region_id == new_rid:
                        key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
                        if key in self._pre_boundaries:
                            return False

    return True


def _check_compass_final(self, board: Board, cell: Cell,
                          region_cells: set[tuple[int, int]]) -> bool:
    r, c = cell.row, cell.col
    for dr, dc, attr in [(-1, 0, "up"), (1, 0, "down"), (0, -1, "left"), (0, 1, "right")]:
        expected = getattr(cell.compass, attr)
        if expected == -1:
            continue
        count = 0
        cr, cc = r + dr, c + dc
        while 0 <= cr < board.height and 0 <= cc < board.width:
            if board.cell(cr, cc).region_id == cell.region_id:
                count += 1
            else:
                break
            cr += dr
            cc += dc
        if count != expected:
            return False
    return True


def _check_global_constraints(self, board: Board,
                               regions: dict[int, set[tuple[int, int]]]) -> bool:
    if not regions:
        return False

    for rid, cells in regions.items():
        for r, c in cells:
            cell = board.cell(r, c)
            if cell.compass is not None and self.puzzle.has_rule("compass"):
                if not self._check_compass_final(board, cell, regions[rid]):
                    return False

    if self.puzzle.has_rule("shape_pool"):
        if not check_rule_shape_pool(self.puzzle, board):
            return False

    if self.puzzle.has_rule("rose_window"):
        if not check_rule_rose_window(self.puzzle, board):
            return False

    if self.puzzle.has_rule("mixed"):
        if not check_rule_mixed(self.puzzle, board):
            return False

    if self.puzzle.has_rule("differentiation"):
        if not check_rule_differentiation(self.puzzle, board):
            return False

    if self.puzzle.has_rule("brick"):
        if not check_rule_brick(self.puzzle, board):
            return False

    if self.puzzle.has_rule("ring"):
        if not check_rule_ring(self.puzzle, board):
            return False

    if self.puzzle.has_rule("inequality"):
        if not check_rule_inequality(self.puzzle, board):
            return False

    if self.puzzle.has_rule("difference"):
        if not check_rule_difference(self.puzzle, board):
            return False

    if self.puzzle.has_rule("watchtower"):
        if not check_rule_watchtower(self.puzzle, board):
            return False

    if self.puzzle.has_rule("fence"):
        if not check_rule_fence(self.puzzle, board):
            return False

    if self.puzzle.has_rule("compass"):
        if not check_rule_compass(self.puzzle, board):
            return False

    if self.puzzle.has_rule("heterogeneous"):
        if not check_rule_heterogeneous(self.puzzle, board):
            return False

    if self.puzzle.has_rule("homogeneous"):
        if not check_rule_homogeneous(self.puzzle, board):
            return False

    if self.puzzle.has_rule("different"):
        if not check_rule_different(self.puzzle, board):
            return False

    if self.puzzle.has_rule("same"):
        if not check_rule_same(self.puzzle, board):
            return False

    if self.puzzle.has_rule("block"):
        if not check_rule_block(self.puzzle, board):
            return False

    if self.puzzle.has_rule("non_block"):
        if not check_rule_non_block(self.puzzle, board):
            return False

    if self.puzzle.has_rule("area"):
        if not check_rule_area(self.puzzle, board):
            return False

    if self.puzzle.has_rule("precise"):
        if not check_rule_precise(self.puzzle, board):
            return False

    if self.puzzle.has_rule("range"):
        if not check_rule_range(self.puzzle, board):
            return False

    if self.puzzle.has_rule("puzzle_piece"):
        if not check_rule_puzzle_piece(self.puzzle, board):
            return False

    if self.puzzle.has_rule("solitary"):
        if not check_rule_solitary(self.puzzle, board):
            return False

    if self._pre_boundaries:
        from src.solver.constraints import check_boundary_consistency
        if not check_boundary_consistency(board):
            return False

    return True
