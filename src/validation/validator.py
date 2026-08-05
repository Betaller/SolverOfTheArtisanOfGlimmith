"""Independent solution validator.

Verifies whether a solver's answer (a ``Board`` with ``region_id`` assigned)
satisfies the puzzle's rules.  This module is deliberately self-contained: it
implements every rule check from scratch and does NOT import anything from
``src.solver``, so a bug in the solver's own rule checks cannot hide a wrong
answer (e.g. a solver that reports "solved" because its internal check is a
stub or shares the same faulty logic).

Usage::

    from src.validation.validator import IndependentValidator
    result = IndependentValidator().validate(puzzle, board)
    result.solved          # bool
    result.rule_results    # {rule_type: bool}
    result.errors          # list[str]
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional

from src.models.board import Board, Cell, Edge, EdgeConstraintType, Shape
from src.models.puzzle import Puzzle


# ─── Self-contained shape helpers (rotations + reflections) ────────────────

def _normalize(cells: frozenset[tuple[int, int]]) -> frozenset[tuple[int, int]]:
    if not cells:
        return frozenset()
    min_r = min(r for r, _ in cells)
    min_c = min(c for _, c in cells)
    return frozenset((r - min_r, c - min_c) for r, c in cells)


def _rot90(cells: frozenset[tuple[int, int]]) -> frozenset[tuple[int, int]]:
    return frozenset((c, -r) for r, c in cells)


def _flip_h(cells: frozenset[tuple[int, int]]) -> frozenset[tuple[int, int]]:
    return frozenset((r, -c) for r, c in cells)


def _all_transforms(cells: frozenset[tuple[int, int]]):
    current = cells
    for _ in range(4):
        yield _normalize(current)
        yield _normalize(_flip_h(current))
        current = _rot90(current)


def _canonical_key(cells: frozenset[tuple[int, int]]) -> str:
    return str(min(sorted((r, c) for r, c in t) for t in _all_transforms(cells)))


def _shape_key(shape: Shape) -> str:
    return _canonical_key(shape.cells)


def _cell_positions(cells: list[Cell]) -> frozenset[tuple[int, int]]:
    return frozenset((c.row, c.col) for c in cells)


# ─── Result types ──────────────────────────────────────────────────────────

@dataclass(slots=True)
class ValidationResult:
    solved: bool
    rule_results: dict[str, bool] = field(default_factory=dict)
    errors: list[str] = field(default_factory=list)


def solution_to_board(puzzle: Puzzle, solution) -> Board:
    """Rebuild a Board carrying the puzzle's clues and the solution's regions.

    Works whether the solution carries an already-populated ``board`` or a
    ``regions`` list (or both).  Used to verify answers independent of which
    solver produced them.
    """
    board = Board(puzzle.height, puzzle.width)
    for c in puzzle.cells:
        dst = board.cell(c.row, c.col)
        dst.number = c.number
        dst.symbol = c.symbol
        dst.shape_pattern = c.shape_pattern
        dst.compass = c.compass
        dst.fence_pattern = c.fence_pattern
        dst.blocked = c.blocked
    assigned = 0
    src = getattr(solution, "board", None)
    if src is not None:
        for c in board.cells():
            if not c.blocked:
                rid = src.cell(c.row, c.col).region_id
                if rid is not None:
                    c.region_id = rid
                    assigned += 1
    if assigned == 0:
        for region in getattr(solution, "regions", None) or []:
            for r, c in region.cells:
                board.cell(r, c).region_id = region.region_id
    return board


# ─── Board analysis helpers (independent implementation) ──────────────────

def _regions(board: Board) -> dict[int, list[Cell]]:
    regions: dict[int, list[Cell]] = {}
    for c in board.cells():
        if c.region_id is not None:
            regions.setdefault(c.region_id, []).append(c)
    return regions


def _is_connected(region: list[Cell]) -> bool:
    positions = {(c.row, c.col) for c in region}
    if not positions:
        return False
    start = next(iter(positions))
    seen: set[tuple[int, int]] = set()
    stack = [start]
    while stack:
        r, c = stack.pop()
        if (r, c) in seen:
            continue
        seen.add((r, c))
        for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
            if (r + dr, c + dc) in positions:
                stack.append((r + dr, c + dc))
    return len(seen) == len(positions)


def _solution_boundary(board: Board, r1: int, c1: int, r2: int, c2: int) -> bool:
    """True if the edge between two cells is a region boundary in the answer.

    两个 blocked 格共享同一空格（同 C++ 的 AREA_BLOCK 值），不构成区域边界——
    否则环纹检查会错误拒绝 blocked 格附近的合法解（如 0678 的官方解）。
    """
    c1c = board.cell(r1, c1)
    c2c = board.cell(r2, c2)
    if c1c.blocked and c2c.blocked:
        return False
    if c1c.blocked or c2c.blocked:
        return True
    return c1c.region_id != c2c.region_id


# ─── Individual rule checks ────────────────────────────────────────────────

def _check_shape_pool(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                      errors: list[str]) -> bool:
    rule = puzzle.get_rule("shape_pool")
    if rule is None:
        return True
    pool = []
    for item in rule.params.get("shapes", []):
        if isinstance(item, Shape):
            pool.append(item)
        else:
            pool.append(Shape(cells=frozenset(tuple(p) for p in item)))
    if not pool:
        return False
    pool_keys = {_shape_key(s) for s in pool}
    ok = True
    for rid, cells in regions.items():
        if _shape_key(Shape(cells=_cell_positions(cells))) not in pool_keys:
            ok = False
            errors.append(f"区域 {rid} 形状不在形状池中")
    return ok


def _check_precise(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                   errors: list[str]) -> bool:
    rule = puzzle.get_rule("precise")
    if rule is None:
        return True
    target = int(rule.params.get("area", 0))
    ok = True
    for rid, cells in regions.items():
        if len(cells) != target:
            ok = False
            errors.append(f"区域 {rid} 面积 {len(cells)} ≠ {target}")
    return ok


def _check_range(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                 errors: list[str]) -> bool:
    rule = puzzle.get_rule("range")
    if rule is None:
        return True
    lo = int(rule.params.get("min", 1))
    hi = int(rule.params.get("max", 1 << 30))
    ok = True
    for rid, cells in regions.items():
        if not (lo <= len(cells) <= hi):
            ok = False
            errors.append(f"区域 {rid} 面积 {len(cells)} 超出 [{lo}, {hi}]")
    return ok


def _check_area(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                errors: list[str]) -> bool:
    if not puzzle.has_rule("area"):
        return True
    ok = True
    for c in board.cells():
        if c.number is not None and c.assigned:
            cells = regions.get(c.region_id)
            if cells is None or len(cells) != c.number:
                ok = False
                errors.append(f"({c.row},{c.col}) 数字 {c.number} 与其区域面积不符")
    return ok


def _check_same(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                errors: list[str]) -> bool:
    if not puzzle.has_rule("same"):
        return True
    keys = {_canonical_key(_cell_positions(cells)) for cells in regions.values()}
    if len(keys) > 1:
        errors.append("所有区域形状应相同")
        return False
    return True


def _check_different(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                     errors: list[str]) -> bool:
    if not puzzle.has_rule("different"):
        return True
    keys = [_canonical_key(_cell_positions(cells)) for cells in regions.values()]
    if len(keys) != len(set(keys)):
        errors.append("存在形状相同的不同区域")
        return False
    return True


def _adjacent_region_pairs(board: Board, regions: dict[int, list[Cell]]):
    seen: set[tuple[int, int]] = set()
    for e in board.edges():
        c1 = board.cell(e.r1, e.c1)
        c2 = board.cell(e.r2, e.c2)
        if c1.blocked or c2.blocked:
            continue
        if c1.region_id != c2.region_id and c1.region_id is not None and c2.region_id is not None:
            key = (min(c1.region_id, c2.region_id), max(c1.region_id, c2.region_id))
            if key not in seen:
                seen.add(key)
                yield c1.region_id, c2.region_id


def _check_mixed(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                 errors: list[str]) -> bool:
    if not puzzle.has_rule("mixed"):
        return True
    for a, b in _adjacent_region_pairs(board, regions):
        ka = _canonical_key(_cell_positions(regions[a]))
        kb = _canonical_key(_cell_positions(regions[b]))
        if ka == kb:
            errors.append(f"相邻区域 {a} 与 {b} 形状相同")
            return False
    return True


def _check_differentiation(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                           errors: list[str]) -> bool:
    if not puzzle.has_rule("differentiation"):
        return True
    for a, b in _adjacent_region_pairs(board, regions):
        if len(regions[a]) == len(regions[b]):
            errors.append(f"相邻区域 {a} 与 {b} 面积相同")
            return False
    return True


def _check_solitary(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                    errors: list[str]) -> bool:
    if not puzzle.has_rule("solitary"):
        return True
    for rid, cells in regions.items():
        clues = sum(1 for c in cells if (
            c.symbol is not None
            or c.compass is not None
            or c.number is not None
            or c.shape_pattern is not None
        ))
        if clues != 1:
            errors.append(f"区域 {rid} 含 {clues} 个标记（应为 1）")
            return False
    return True


def _is_rectangle(cells: list[Cell]) -> bool:
    positions = _cell_positions(cells)
    if not positions:
        return False
    min_r = min(r for r, _ in positions)
    max_r = max(r for r, _ in positions)
    min_c = min(c for _, c in positions)
    max_c = max(c for _, c in positions)
    return len(positions) == (max_r - min_r + 1) * (max_c - min_c + 1)


def _check_block(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                 errors: list[str]) -> bool:
    if not puzzle.has_rule("block"):
        return True
    for rid, cells in regions.items():
        if not _is_rectangle(cells):
            errors.append(f"区域 {rid} 不是矩形")
            return False
    return True


def _check_non_block(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                     errors: list[str]) -> bool:
    if not puzzle.has_rule("non_block"):
        return True
    for rid, cells in regions.items():
        if _is_rectangle(cells):
            errors.append(f"区域 {rid} 是矩形（禁止）")
            return False
    return True


def _check_puzzle_piece(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                        errors: list[str]) -> bool:
    if not puzzle.has_rule("puzzle_piece"):
        return True
    for c in board.cells():
        if c.shape_pattern is not None and c.assigned:
            region_cells = regions.get(c.region_id)
            if region_cells is None:
                return False
            if _canonical_key(_cell_positions(region_cells)) != _shape_key(c.shape_pattern):
                errors.append(f"({c.row},{c.col}) 所在区域形状与标记不一致")
                return False
    return True


def _check_fence(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                 errors: list[str]) -> bool:
    if not puzzle.has_rule("fence"):
        return True
    for c in board.cells():
        if c.fence_pattern is None or not c.assigned:
            continue
        bits: list[bool] = []
        for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
            nr, nc = c.row + dr, c.col + dc
            if 0 <= nr < board.height and 0 <= nc < board.width:
                nbr = board.cell(nr, nc)
                bits.append(nbr.blocked or (nbr.assigned and nbr.region_id != c.region_id))
            else:
                bits.append(True)
        # 3x3 pattern: center + up/down/left/right boundary bits
        pattern = {(1, 1)}
        for bit, (r, cc) in zip(bits, [(0, 1), (2, 1), (1, 0), (1, 2)]):
            if bit:
                pattern.add((r, cc))
        if _canonical_key(frozenset(pattern)) != _shape_key(c.fence_pattern):
            errors.append(f"({c.row},{c.col}) 围栏图案不匹配")
            return False
    return True


def _check_compass(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                   errors: list[str]) -> bool:
    if not puzzle.has_rule("compass"):
        return True
    for c in board.cells():
        if c.compass is None or not c.assigned:
            continue
        region_cells = regions.get(c.region_id)
        if region_cells is None:
            return False
        positions = {(cell.row, cell.col) for cell in region_cells}
        r, col = c.row, c.col
        for dr, dc, attr in [(-1, 0, "up"), (1, 0, "down"), (0, -1, "left"), (0, 1, "right")]:
            expected = getattr(c.compass, attr)
            if expected == -1:
                continue
            count = 0
            for (rr, cc) in positions:
                if rr == r and cc == col:
                    continue
                if dr == -1 and rr < r:
                    count += 1
                elif dr == 1 and rr > r:
                    count += 1
                elif dc == -1 and cc < col:
                    count += 1
                elif dc == 1 and cc > col:
                    count += 1
            if count != expected:
                errors.append(f"({r},{col}) 罗盘 {attr} 应为 {expected}，实际 {count}")
                return False
    return True


def _check_edge_constraints(puzzle: Puzzle, board: Board,
                            regions: dict[int, list[Cell]], errors: list[str]) -> bool:
    ok = True
    for e in puzzle.edges:
        if e.constraint is None:
            continue
        c1 = board.cell(e.r1, e.c1)
        c2 = board.cell(e.r2, e.c2)
        if c1.blocked or c2.blocked:
            ok = False
            errors.append(f"约束边 ({e.r1},{e.c1})-({e.r2},{e.c2}) 邻接障碍格")
            continue
        if not (c1.assigned and c2.assigned):
            ok = False
            errors.append(f"约束边 ({e.r1},{e.c1})-({e.r2},{e.c2}) 两端未分配")
            continue
        if c1.region_id == c2.region_id:
            ok = False
            errors.append(f"约束边 ({e.r1},{e.c1})-({e.r2},{e.c2}) 两端同一区域")
            continue
        ca = regions[c1.region_id]
        cb = regions[c2.region_id]
        ct = e.constraint.type
        if ct == EdgeConstraintType.INEQUALITY:
            reversed_dir = e.constraint.value == 1
            if reversed_dir:
                if len(cb) >= len(ca):
                    ok = False
                    errors.append(f"不等号边 ({e.r1},{e.c1})-({e.r2},{e.c2}) 方向不满足")
            else:
                if len(ca) >= len(cb):
                    ok = False
                    errors.append(f"不等号边 ({e.r1},{e.c1})-({e.r2},{e.c2}) 方向不满足")
        elif ct == EdgeConstraintType.DIFFERENCE:
            if abs(len(ca) - len(cb)) != (e.constraint.value or 0):
                ok = False
                errors.append(f"差值边 ({e.r1},{e.c1})-({e.r2},{e.c2}) 面积差不符合")
        elif ct == EdgeConstraintType.HETEROGENEOUS:
            if _canonical_key(_cell_positions(ca)) == _canonical_key(_cell_positions(cb)):
                ok = False
                errors.append(f"异生边 ({e.r1},{e.c1})-({e.r2},{e.c2}) 两侧形状相同")
        elif ct == EdgeConstraintType.HOMOGENEOUS:
            if _canonical_key(_cell_positions(ca)) != _canonical_key(_cell_positions(cb)):
                ok = False
                errors.append(f"双生边 ({e.r1},{e.c1})-({e.r2},{e.c2}) 两侧形状不同")
    return ok


def _check_watchtower(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                      errors: list[str]) -> bool:
    if not puzzle.has_rule("watchtower"):
        return True
    for v in board.vertices():
        if v.watchtower is not None:
            cells = board.cells_surrounding_vertex(v.row, v.col)
            distinct = {c.region_id for c in cells if c.assigned}
            if len(distinct) != v.watchtower:
                errors.append(f"顶点 ({v.row},{v.col}) 望塔 {v.watchtower} ≠ {len(distinct)} 个区域")
                return False
    return True


def _check_brick(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                 errors: list[str]) -> bool:
    if not puzzle.has_rule("brick"):
        return True
    for v in board.vertices():
        # 4 路交叉 = 4 个**区域**在顶点相会。顶点周围若有 blocked（空格），
        # 空格不是区域，不可能构成 4 路交叉——跳过（与 C++ check_tatami 一致，
        # 后者把所有 blocked 视为同一个非区域值，永远不会计数为不同区域）。
        if any(c.blocked for c in board.cells_surrounding_vertex(v.row, v.col)):
            continue
        count = sum(1 for e in board.edges_surrounding_vertex(v.row, v.col)
                    if _solution_boundary(board, e.r1, e.c1, e.r2, e.c2))
        if count == 4:
            errors.append(f"顶点 ({v.row},{v.col}) 出现 4 路交叉（砖纹禁止）")
            return False
    return True


def _check_ring(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                errors: list[str]) -> bool:
    if not puzzle.has_rule("ring"):
        return True
    for v in board.vertices():
        count = sum(1 for e in board.edges_surrounding_vertex(v.row, v.col)
                    if _solution_boundary(board, e.r1, e.c1, e.r2, e.c2))
        if count == 3:
            errors.append(f"顶点 ({v.row},{v.col}) 出现 3 路交叉（环纹禁止）")
            return False
    return True


def _check_rose_window(puzzle: Puzzle, board: Board, regions: dict[int, list[Cell]],
                       errors: list[str]) -> bool:
    if not puzzle.has_rule("rose_window"):
        return True
    rule = puzzle.get_rule("rose_window")
    symbol_types = rule.params.get("symbol_types", []) if rule else []
    if not symbol_types:
        symbol_types = sorted({c.symbol for c in board.cells() if c.symbol is not None})
    if not symbol_types:
        errors.append("玫瑰窗规则缺少符号类型")
        return False
    types = set(symbol_types)
    counts: dict[str, int] = {}
    for c in board.cells():
        if c.symbol is not None:
            if c.symbol not in types:
                errors.append(f"存在规则外的符号 {c.symbol}")
                return False
            counts[c.symbol] = counts.get(c.symbol, 0) + 1
    if len(set(counts.values())) != 1:
        errors.append("各符号出现次数不一致")
        return False
    m = next(iter(counts.values()))
    if len(regions) != m:
        errors.append(f"区域数 {len(regions)} ≠ 每符号次数 {m}")
        return False
    for rid, cells in regions.items():
        syms = {c.symbol for c in cells if c.symbol is not None}
        if syms != types:
            errors.append(f"区域 {rid} 未包含全部符号类型")
            return False
    return True


_RULE_CHECKS = {
    "shape_pool": _check_shape_pool,
    "rose_window": _check_rose_window,
    "precise": _check_precise,
    "range": _check_range,
    "area": _check_area,
    "same": _check_same,
    "different": _check_different,
    "mixed": _check_mixed,
    "differentiation": _check_differentiation,
    "solitary": _check_solitary,
    "block": _check_block,
    "non_block": _check_non_block,
    "puzzle_piece": _check_puzzle_piece,
    "fence": _check_fence,
    "compass": _check_compass,
    "heterogeneous": _check_edge_constraints,
    "homogeneous": _check_edge_constraints,
    "inequality": _check_edge_constraints,
    "difference": _check_edge_constraints,
    "watchtower": _check_watchtower,
    "brick": _check_brick,
    "ring": _check_ring,
}


class IndependentValidator:
    """Validates a solved board against the puzzle, without using solver logic."""

    def validate(self, puzzle: Puzzle, board: Board) -> ValidationResult:
        errors: list[str] = []

        # 1) every fillable cell is assigned to exactly one region
        regions = _regions(board)
        unassigned = [c for c in board.cells() if not c.blocked and c.region_id is None]
        if unassigned:
            errors.append(f"{len(unassigned)} 个可填格未分配区域")

        # 2) every region is connected
        for rid, cells in regions.items():
            if not _is_connected(cells):
                errors.append(f"区域 {rid} 不连通")

        # 3) pre-drawn boundaries separate regions
        for e in puzzle.edges:
            if e.is_boundary:
                c1 = board.cell(e.r1, e.c1)
                c2 = board.cell(e.r2, e.c2)
                if not c1.blocked and not c2.blocked:
                    if c1.region_id is None or c2.region_id is None:
                        errors.append(f"预画边界 ({e.r1},{e.c1})-({e.r2},{e.c2}) 邻接未分配格")
                    elif c1.region_id == c2.region_id:
                        errors.append(f"预画边界 ({e.r1},{e.c1})-({e.r2},{e.c2}) 两侧同一区域")

        # 4) per-rule checks (implemented independently)
        rule_results: dict[str, bool] = {}
        active_rules = {r.type for r in puzzle.rules}
        for rule_type in active_rules:
            checker = _RULE_CHECKS.get(rule_type)
            if checker is not None:
                rule_results[rule_type] = bool(checker(puzzle, board, regions, errors))

        solved = not errors and all(
            c.region_id is not None for c in board.cells() if not c.blocked
        )
        return ValidationResult(solved=solved, rule_results=rule_results, errors=errors)
