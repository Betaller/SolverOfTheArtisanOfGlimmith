from __future__ import annotations

from collections import Counter
from src.models.board import Board, Cell, Edge, EdgeConstraintType, Shape
from src.models.puzzle import Puzzle
from src.solver.shapes import (
    canonical_key, shapes_equal, match_shape_pool,
    is_rectangle, shape_key,
)


def get_region_cells(board: Board) -> dict[int, list[Cell]]:
    regions: dict[int, list[Cell]] = {}
    for c in board.cells():
        if c.region_id is not None:
            regions.setdefault(c.region_id, []).append(c)
    return regions


def get_region_shape(cells: list[Cell]) -> Shape:
    positions = [(c.row, c.col) for c in cells]
    return Shape(cells=frozenset(positions))


def check_region_connectivity(board: Board) -> bool:
    for rid, cells in get_region_cells(board).items():
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
            return False
    return True


def check_boundary_consistency(board: Board) -> bool:
    for e in board.edges():
        c1 = board.cell(e.r1, e.c1)
        c2 = board.cell(e.r2, e.c2)
        if c1.assigned and c2.assigned:
            expected_boundary = c1.region_id != c2.region_id
        elif c1.blocked or c2.blocked:
            expected_boundary = True
        else:
            expected_boundary = False
        if e.is_boundary and not expected_boundary:
            return False
    return True


def check_rule_shape_pool(puzzle: Puzzle, board: Board) -> bool:
    rule = puzzle.get_rule("shape_pool")
    if rule is None:
        return True
    pool: list[Shape] = rule.params.get("shapes", [])
    if not pool:
        return False
    for _, cells in get_region_cells(board).items():
        region_shape = get_region_shape(cells)
        if match_shape_pool(region_shape, pool) is None:
            return False
    return True


def check_rule_rose_window(puzzle: Puzzle, board: Board) -> bool:
    rule = puzzle.get_rule("rose_window")
    if rule is None:
        return True
    symbol_types: list[str] = rule.params.get("symbol_types", [])
    if not symbol_types:
        return False
    N = len(symbol_types)
    
    symbol_counts: Counter[str] = Counter()
    symbol_positions: dict[str, list[tuple[int, int]]] = {s: [] for s in symbol_types}
    for c in board.cells():
        if c.symbol is not None:
            if c.symbol not in symbol_types:
                return False
            symbol_counts[c.symbol] += 1
            symbol_positions[c.symbol].append((c.row, c.col))
    
    if len(set(symbol_counts.values())) != 1:
        return False
    M = next(iter(symbol_counts.values()))
    
    regions = get_region_cells(board)
    if len(regions) != M:
        return False
    
    for rid, cells in regions.items():
        region_symbols = {c.symbol for c in cells if c.symbol is not None}
        if region_symbols != set(symbol_types):
            return False
    
    return True


def check_edge_constraint_type(board: Board, etype: EdgeConstraintType) -> bool:
    for e in board.edges():
        if e.constraint is not None and e.constraint.type == etype:
            c1 = board.cell(e.r1, e.c1)
            c2 = board.cell(e.r2, e.c2)
            if not c1.assigned or not c2.assigned:
                return False
            if c1.region_id == c2.region_id:
                return False
            cells1 = board.get_region_cells(c1.region_id)
            cells2 = board.get_region_cells(c2.region_id)
            shape1 = get_region_shape(cells1)
            shape2 = get_region_shape(cells2)
            eq = shapes_equal(shape1, shape2)
            if etype == EdgeConstraintType.HETEROGENEOUS and eq:
                return False
            if etype == EdgeConstraintType.HOMOGENEOUS and not eq:
                return False
    return True


def check_rule_heterogeneous(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("heterogeneous"):
        return True
    return check_edge_constraint_type(board, EdgeConstraintType.HETEROGENEOUS)


def check_rule_homogeneous(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("homogeneous"):
        return True
    return check_edge_constraint_type(board, EdgeConstraintType.HOMOGENEOUS)


def check_rule_precise(puzzle: Puzzle, board: Board) -> bool:
    rule = puzzle.get_rule("precise")
    if rule is None:
        return True
    target = rule.params.get("area", 0)
    for _, cells in get_region_cells(board).items():
        if len(cells) != target:
            return False
    return True


def check_rule_puzzle_piece(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("puzzle_piece"):
        return True
    for c in board.cells():
        if c.shape_pattern is not None and c.assigned:
            cells = board.get_region_cells(c.region_id)
            region_shape = get_region_shape(cells)
            if not shapes_equal(region_shape, c.shape_pattern):
                return False
    return True


def check_rule_mixed(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("mixed"):
        return True
    for e in board.edges():
        c1 = board.cell(e.r1, e.c1)
        c2 = board.cell(e.r2, e.c2)
        if c1.assigned and c2.assigned and c1.region_id != c2.region_id:
            cells1 = board.get_region_cells(c1.region_id)
            cells2 = board.get_region_cells(c2.region_id)
            shape1 = get_region_shape(cells1)
            shape2 = get_region_shape(cells2)
            if shapes_equal(shape1, shape2):
                return False
    return True


def check_rule_area(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("area"):
        return True
    for c in board.cells():
        if c.number is not None and c.assigned:
            cells = board.get_region_cells(c.region_id)
            if len(cells) != c.number:
                return False
    return True


def check_rule_same(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("same"):
        return True
    regions = get_region_cells(board)
    shape_keys: set[str] = set()
    for _, cells in regions.items():
        shape_keys.add(canonical_key(get_region_shape(cells).cells))
    return len(shape_keys) <= 1


def check_rule_range(puzzle: Puzzle, board: Board) -> bool:
    rule = puzzle.get_rule("range")
    if rule is None:
        return True
    min_area = rule.params.get("min", 0)
    max_area = rule.params.get("max", 999)
    for _, cells in get_region_cells(board).items():
        if not (min_area <= len(cells) <= max_area):
            return False
    return True


def check_rule_fence(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("fence"):
        return True
    for c in board.cells():
        if c.fence_pattern is None or not c.assigned:
            continue
        edge_bits: list[bool] = []
        for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
            nr, nc = c.row + dr, c.col + dc
            if 0 <= nr < board.height and 0 <= nc < board.width:
                neighbor = board.cell(nr, nc)
                is_boundary = neighbor.assigned and neighbor.region_id != c.region_id
            else:
                is_boundary = True
            edge_bits.append(is_boundary)
        
        fence_shape = Shape(cells=frozenset(
            (r, c) for r in range(3) for c in range(3) if _fence_bit(edge_bits, r, c)
        ))
        if not shapes_equal(fence_shape, c.fence_pattern):
            return False
    return True


def _fence_bit(edge_bits: list[bool], r: int, c: int) -> bool:
    center = (r == 1 and c == 1)
    if center:
        return True
    if r == 0 and c == 1:
        return edge_bits[0]
    if r == 2 and c == 1:
        return edge_bits[1]
    if r == 1 and c == 0:
        return edge_bits[2]
    if r == 1 and c == 2:
        return edge_bits[3]
    return False


def check_rule_different(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("different"):
        return True
    regions = get_region_cells(board)
    shape_keys_list: list[str] = []
    for _, cells in regions.items():
        shape_keys_list.append(canonical_key(get_region_shape(cells).cells))
    return len(shape_keys_list) == len(set(shape_keys_list))


def check_rule_solitary(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("solitary"):
        return True
    for _, cells in get_region_cells(board).items():
        symbols = [c.symbol for c in cells if c.symbol is not None]
        if len(symbols) != 1:
            return False
    return True


def check_rule_block(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("block"):
        return True
    for _, cells in get_region_cells(board).items():
        shape = get_region_shape(cells)
        if not is_rectangle(shape):
            return False
    return True


def check_rule_non_block(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("non_block"):
        return True
    for _, cells in get_region_cells(board).items():
        shape = get_region_shape(cells)
        if is_rectangle(shape):
            return False
    return True


def check_rule_differentiation(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("differentiation"):
        return True
    for e in board.edges():
        c1 = board.cell(e.r1, e.c1)
        c2 = board.cell(e.r2, e.c2)
        if c1.assigned and c2.assigned and c1.region_id != c2.region_id:
            cells1 = board.get_region_cells(c1.region_id)
            cells2 = board.get_region_cells(c2.region_id)
            if len(cells1) == len(cells2):
                return False
    return True


def count_boundary_edges_at_vertex(board: Board, vr: int, vc: int) -> int:
    count = 0
    for e in board.edges_surrounding_vertex(vr, vc):
        if e.is_boundary:
            count += 1
    return count


def check_rule_brick(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("brick"):
        return True
    for v in board.vertices():
        if count_boundary_edges_at_vertex(board, v.row, v.col) == 4:
            return False
    return True


def check_rule_ring(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("ring"):
        return True
    for v in board.vertices():
        if count_boundary_edges_at_vertex(board, v.row, v.col) == 3:
            return False
    return True


def check_rule_inequality(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("inequality"):
        return True
    for e in board.edges():
        if e.constraint is not None and e.constraint.type == EdgeConstraintType.INEQUALITY:
            c1 = board.cell(e.r1, e.c1)
            c2 = board.cell(e.r2, e.c2)
            if not c1.assigned or not c2.assigned:
                return False
            if c1.region_id == c2.region_id:
                return False
            cells1 = board.get_region_cells(c1.region_id)
            cells2 = board.get_region_cells(c2.region_id)
            area1, area2 = len(cells1), len(cells2)
            reversed_dir = e.constraint.value == 1
            if reversed_dir:
                if area2 >= area1:
                    return False
            else:
                if area1 >= area2:
                    return False
    return True


def check_rule_difference(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("difference"):
        return True
    for e in board.edges():
        if e.constraint is not None and e.constraint.type == EdgeConstraintType.DIFFERENCE:
            c1 = board.cell(e.r1, e.c1)
            c2 = board.cell(e.r2, e.c2)
            if not c1.assigned or not c2.assigned:
                return False
            if c1.region_id == c2.region_id:
                return False
            cells1 = board.get_region_cells(c1.region_id)
            cells2 = board.get_region_cells(c2.region_id)
            if abs(len(cells1) - len(cells2)) != e.constraint.value:
                return False
    return True


def check_rule_watchtower(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("watchtower"):
        return True
    for v in board.vertices():
        if v.watchtower is not None:
            cells = board.cells_surrounding_vertex(v.row, v.col)
            distinct_regions = {c.region_id for c in cells if c.assigned}
            if len(distinct_regions) != v.watchtower:
                return False
    return True


def check_rule_compass(puzzle: Puzzle, board: Board) -> bool:
    if not puzzle.has_rule("compass"):
        return True
    for c in board.cells():
        if c.compass is None or not c.assigned:
            continue
        for dr, dc, attr in [(-1, 0, "up"), (1, 0, "down"), (0, -1, "left"), (0, 1, "right")]:
            expected = getattr(c.compass, attr)
            if expected == -1:
                continue
            count = 0
            r, col = c.row + dr, c.col + dc
            while 0 <= r < board.height and 0 <= col < board.width:
                neighbor = board.cell(r, col)
                if neighbor.assigned and neighbor.region_id == c.region_id:
                    count += 1
                else:
                    break
                r += dr
                col += dc
            if count != expected:
                return False
    return True


RULE_CHECKERS = {
    "shape_pool": check_rule_shape_pool,
    "rose_window": check_rule_rose_window,
    "heterogeneous": check_rule_heterogeneous,
    "homogeneous": check_rule_homogeneous,
    "precise": check_rule_precise,
    "puzzle_piece": check_rule_puzzle_piece,
    "mixed": check_rule_mixed,
    "area": check_rule_area,
    "same": check_rule_same,
    "range": check_rule_range,
    "fence": check_rule_fence,
    "different": check_rule_different,
    "solitary": check_rule_solitary,
    "block": check_rule_block,
    "non_block": check_rule_non_block,
    "differentiation": check_rule_differentiation,
    "brick": check_rule_brick,
    "ring": check_rule_ring,
    "inequality": check_rule_inequality,
    "difference": check_rule_difference,
    "watchtower": check_rule_watchtower,
    "compass": check_rule_compass,
}
