"""
Convert the official puzzle archive (third_party/archiveofglimmith.github.io/puzzles.json)
into SolverOfTheArtisanOfGlimmith JSON format, overwriting puzzles/official/Zone1-3.

Usage:
    python scripts/convert_archive.py            # convert all Zone1-3 puzzles
    python scripts/convert_archive.py --dry-run  # only report, don't write
"""

from __future__ import annotations

import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from src.io.puzzle_codec import dict_to_puzzle
from src.models.board import CompassClue, Shape

ARCHIVE_PATH = os.path.join("third_party", "archiveofglimmith.github.io", "puzzles.json")
OUT_ROOT = os.path.join("puzzles", "official")

# F-value (palisade / fence) -> 3x3 boundary pattern, empirically derived from
# the archive's own solutions: F0=0 boundaries, F1=1, F2=2 opposite,
# F3=3, F4=4, F7=2 adjacent. The check uses rotation/reflection symmetry, so
# orientation choices don't matter.
FENCE_PATTERNS: dict[str, frozenset[tuple[int, int]]] = {
    "F0": frozenset({(1, 1)}),
    "F1": frozenset({(0, 1), (1, 1)}),
    "F2": frozenset({(0, 1), (2, 1), (1, 1)}),
    "F3": frozenset({(0, 1), (2, 1), (1, 0), (1, 1)}),
    "F4": frozenset({(0, 1), (2, 1), (1, 0), (1, 2), (1, 1)}),
    "F7": frozenset({(0, 1), (1, 0), (1, 1)}),
}

_COMPASS_RE = re.compile(r"R\d*")

# (r1, c1, r2, c2) edge endpoint tuple.
_Edge = tuple[int, int, int, int]

# Edge glyph -> (constraint type or None, value or None).  Every entry also
# forces a boundary.  Digits / "-<digit>" are handled separately (variable
# value), so they are absent here.
_EDGE_GLYPH_RULES: dict[str, tuple[str | None, int | None]] = {
    "#": (None, None),
    "##": (None, None),
    "==": ("homogeneous", None),
    "=": ("homogeneous", None),
    "!!": ("heterogeneous", None),
    "!": ("heterogeneous", None),
    "^^": ("inequality", None),
    "<": ("inequality", None),
    "^": ("inequality", None),
    "vv": ("inequality", 1),
    ">": ("inequality", 1),
    "v": ("inequality", 1),
}

# Archive flag field -> emitted rule type.
_FLAG_RULES: tuple[tuple[str, str], ...] = (
    ("all_shapes_same", "same"),
    ("all_shapes_different", "different"),
    ("adjacent_shapes_different", "mixed"),
    ("adjacent_sizes_different", "differentiation"),
    ("only_rectangles", "block"),
    ("no_rectangles", "non_block"),
    ("no_4_way_intersections", "brick"),
    ("no_3_way_intersections", "ring"),
)


def parse_shapes(p: dict) -> dict[int, Shape]:
    """Map shape id -> normalized Shape from the archive 'shapes' list."""
    result: dict[int, Shape] = {}
    for shape in p.get("shapes", []):
        grid = shape.get("grid", [])
        cells = {(r, c) for r, row in enumerate(grid) for c, ch in enumerate(row) if ch == "#"}
        if cells:
            min_r = min(r for r, _ in cells)
            min_c = min(c for _, c in cells)
            result[int(shape["id"])] = Shape(
                cells=frozenset((r - min_r, c - min_c) for r, c in cells)
            )
    return result


def _compass_from_str(s: str) -> CompassClue:
    values = {"U": -1, "D": -1, "L": -1, "R": -1}
    for m in re.finditer(r"([UDLR])(\d*)", s):
        dir_, digits = m.group(1), m.group(2)
        values[dir_] = int(digits) if digits else -1
    return CompassClue(up=values["U"], down=values["D"], left=values["L"], right=values["R"])


def _shape_list(shape: Shape) -> list[list[int]]:
    return sorted((r, c) for r, c in shape.cells)


# ---------------------------------------------------------------------------
# Archive solution partition (union-find over fillable, non-boundary-adjacent cells)
# ---------------------------------------------------------------------------
class _GridDSU:
    """Union-find over the grid's fillable cells."""

    def __init__(self, h: int, w: int, blocked: set[tuple[int, int]]) -> None:
        self.comp = {(r, c): (r, c) for r in range(h) for c in range(w) if (r, c) not in blocked}

    def find(self, x: tuple[int, int]) -> tuple[int, int]:
        while self.comp[x] != x:
            self.comp[x] = self.comp[self.comp[x]]
            x = self.comp[x]
        return x

    def union(self, a: tuple[int, int], b: tuple[int, int]) -> None:
        ra, rb = self.find(a), self.find(b)
        if ra != rb:
            self.comp[ra] = rb

    def partitions(self) -> list[frozenset[tuple[int, int]]]:
        parts: dict[tuple[int, int], list[tuple[int, int]]] = {}
        for cell in self.comp:
            parts.setdefault(self.find(cell), []).append(cell)
        return [frozenset(cells) for cells in parts.values()]


def _archive_horizontal_edges(lines: list[str], width: int, height: int) -> set[_Edge]:
    edges: set[_Edge] = set()
    for r in range(height + 1):
        if 2 * r >= len(lines):
            break
        row = lines[2 * r]
        for c in range(width):
            if row[3 * c + 1 : 3 * c + 3].strip():
                edges.add((r - 1, c, r, c))
    return edges


def _archive_vertical_edges(lines: list[str], width: int, height: int) -> set[_Edge]:
    edges: set[_Edge] = set()
    for r in range(height):
        if 2 * r + 1 >= len(lines):
            break
        row = lines[2 * r + 1]
        for c in range(width + 1):
            if row[3 * c] == "#":
                edges.add((r, c - 1, r, c))
    return edges


def _connect_adjacent(
    dsu: _GridDSU, height: int, width: int, blocked: set[tuple[int, int]], edges: set[_Edge]
) -> None:
    for r in range(height):
        for c in range(width):
            if (r, c) in blocked:
                continue
            for dr, dc in ((1, 0), (0, 1)):
                nr, nc = r + dr, c + dc
                same_block = (nr, nc) in blocked
                same_edge = (r, c, nr, nc) in edges
                if nr < height and nc < width and not same_block and not same_edge:
                    dsu.union((r, c), (nr, nc))


def _archive_partition(p: dict, blocked: set[tuple[int, int]]) -> list[frozenset[tuple[int, int]]]:
    """Parse the archive's compact solution into a region partition.

    Returns a list of frozensets of (row, col), excluding blocked cells.
    """
    width, height = int(p["width"]), int(p["height"])
    sol = p.get("solution") or []
    if not sol:
        return []
    lines = [ln.ljust(3 * width + 1, " ") for ln in sol]
    h_edges = _archive_horizontal_edges(lines, width, height)
    v_edges = _archive_vertical_edges(lines, width, height)
    edges = h_edges | v_edges
    dsu = _GridDSU(height, width, blocked)
    _connect_adjacent(dsu, height, width, blocked, edges)
    return dsu.partitions()


# ---------------------------------------------------------------------------
# Cell-row tokenisation
# ---------------------------------------------------------------------------
def _parse_cell_token(first: str, row: str, pos: int, shape_ids: set[int]) -> tuple[str, int]:
    """Parse a single cell's content starting at *row[pos]*; return (token, new_pos)."""
    if first == "U":
        m = _COMPASS_RE.search(row[pos:])
        end = pos + m.end() if m else pos + 2
        return row[pos:end], end
    if first == "S":
        # greedy: read S + all consecutive digits, then back off so the
        # id exists in the shape table (handles multi-digit ids like S10)
        m = re.match(r"S(\d+)", row[pos:])
        if m:
            digits = m.group(1)
            n = len(digits)
            while n > 1 and int(digits[:n]) not in shape_ids:
                n -= 1
            end = pos + 1 + n
            return row[pos:end], end
        return row[pos : pos + 2], pos + 2
    return row[pos : pos + 2], pos + 2


def _parse_cell_row(row: str, width: int, shape_ids: set[int]):
    """Greedy cell-row parser (mirrors the archive renderer, with S-id support).

    Returns (cells, walls): lists of length `width` and `width+1`.
    """
    cells: list[str] = []
    walls: list[str] = []
    pos = 0
    for c in range(width + 1):
        walls.append(row[pos] if pos < len(row) else " ")
        pos += 1
        if c < width:
            first = row[pos] if pos < len(row) else " "
            token, pos = _parse_cell_token(first, row, pos, shape_ids)
            cells.append(token)
    while len(cells) < width:
        cells.append("  ")
    while len(walls) < width + 1:
        walls.append(" ")
    return cells, walls


# ---------------------------------------------------------------------------
# Cell content dispatch (one cell)
# ---------------------------------------------------------------------------
def _apply_cell_content(
    cell: dict,
    content: str,
    is_spr: bool,
    shapes: dict[int, Shape],
    p_symbols: dict[str, int],
    p_symbol_positions: dict[tuple[int, int], str],
    r: int,
    c: int,
) -> str | None:
    """Decode one cell's clue, mutating *cell* in place.

    Returns a feature tag among ``"numbers"``, ``"shape_pattern"``,
    ``"fence"``, ``"compass"`` when the clue carries that property (so the
    caller can raise the matching rule flag), or ``None`` otherwise.
    """
    if re.fullmatch(r"\d\d", content):
        cell["number"] = int(content)
        cell.pop("symbol", None)
        return "numbers"
    if re.fullmatch(r"S\d+", content):
        sid = int(content[1:])
        if sid in shapes:
            cell["shape_pattern"] = _shape_list(shapes[sid])
            cell.pop("symbol", None)
            return "shape_pattern"
        return None
    if content in FENCE_PATTERNS:
        cell["fence_pattern"] = _shape_list(Shape(cells=FENCE_PATTERNS[content]))
        cell.pop("symbol", None)
        return "fence"
    if re.fullmatch(r"P[1-9]", content):
        p_symbols[content] = p_symbols.get(content, 0) + 1
        p_symbol_positions[(r, c)] = content
        if not is_spr:
            cell["symbol"] = content
        return None
    if content[0] in "UDLR" and re.fullmatch(r"[UDLR\d]+", content):
        comp = _compass_from_str(content)
        cell["compass"] = {
            "up": comp.up,
            "down": comp.down,
            "left": comp.left,
            "right": comp.right,
        }
        cell.pop("symbol", None)
        return "compass"
    return None


# ---------------------------------------------------------------------------
# Rose-window detection
# ---------------------------------------------------------------------------
def _rose_fallback_sym_pos(p: dict, p_symbols: dict[str, int]) -> dict[tuple[int, int], str]:
    """Fixed-stride read of symbol positions (only correct without variable-width cells)."""
    width, height = int(p["width"]), int(p["height"])
    sym_pos: dict[tuple[int, int], str] = {}
    for r in range(height):
        row = (p["puzzle_grid"][2 * r + 1] or "").ljust(3 * width + 1, " ")
        for c in range(width):
            content = row[3 * c + 1 : 3 * c + 3]
            if content in p_symbols:
                sym_pos[(r, c)] = content
    return sym_pos


def _rose_partition_matches(
    part: list[frozenset[tuple[int, int]]],
    types: set[str],
    sym_pos: dict[tuple[int, int], str],
) -> bool:
    for cells in part:
        syms = {sym_pos[pos] for pos in cells if pos in sym_pos}
        if syms != types or len(syms) != len(types):
            return False
    return True


def _is_rose_window(
    p: dict,
    p_symbols: dict[str, int],
    blocked: set[tuple[int, int]],
    sym_pos: dict[tuple[int, int], str] | None = None,
) -> list[str] | None:
    """Return sorted rose symbol types if the official solution confirms a
    rose-window pattern (N types x M occurrences, M regions each containing
    all N types), else None.

    ``sym_pos`` must come from the greedy cell parser (_parse_cell_row): reading
    the raw grid at a fixed 2-char stride is WRONG when a variable-width cell
    (compass U…, S-shape) precedes a P-cell and shifts its position.
    """
    if not p_symbols or len(set(p_symbols.values())) != 1:
        return None
    m = next(iter(p_symbols.values()))
    types = set(p_symbols)
    if not p.get("solution"):
        # no official answer to confirm; only accept the multi-type case
        return sorted(p_symbols) if len(p_symbols) >= 2 else None
    part = _archive_partition(p, blocked)
    if len(part) != m:
        return None
    if sym_pos is None:
        sym_pos = _rose_fallback_sym_pos(p, p_symbols)
    if not _rose_partition_matches(part, types, sym_pos):
        return None
    return sorted(p_symbols)


# ---------------------------------------------------------------------------
# Per-row processing for _parse_puzzle
# ---------------------------------------------------------------------------
def _emit_cell(
    cell: dict,
    content: str,
    is_spr: bool,
    shapes: dict[int, Shape],
    p_symbols: dict[str, int],
    p_symbol_positions: dict[tuple[int, int], str],
    r: int,
    c: int,
    fillable: list,
) -> str | None:
    """Process one cell's raw content. Returns the feature tag (or None)."""
    s = content.strip()
    if not s or content == "  ":
        cell["blocked"] = True
        fillable[r][c] = False
        return None
    if content == "..":
        return None
    # clues keep their structured form; in 1-symbol-per-region
    # puzzles every clue also counts as a symbol marker
    if is_spr:
        cell["symbol"] = content
    return _apply_cell_content(cell, content, is_spr, shapes, p_symbols, p_symbol_positions, r, c)


def _parse_puzzle_cell_rows(
    lines: list[str],
    height: int,
    width: int,
    shape_ids: set[int],
    is_spr: bool,
    shapes: dict[int, Shape],
    cell_map: dict,
    fillable: list,
    edge_map: dict,
    p_symbols: dict[str, int],
    p_symbol_positions: dict[tuple[int, int], str],
    flags: dict[str, bool],
) -> None:
    for r in range(height):
        cells_row, walls = _parse_cell_row(lines[2 * r + 1], width, shape_ids)
        for c in range(width):
            cell = cell_map[(r, c)]
            feature = _emit_cell(
                cell, cells_row[c], is_spr, shapes, p_symbols, p_symbol_positions, r, c, fillable
            )
            if feature:
                flags[feature] = True
        for c in range(width + 1):
            ch = walls[c]
            if c == 0 or c == width:
                continue
            _apply_edge(edge_map[(r, c - 1, r, c)], ch, cell_map, fillable, r, c - 1, r, c)


def _parse_puzzle_wall_rows(
    lines: list[str], height: int, width: int, edge_map: dict, fillable: list, cell_map: dict
) -> None:
    for r in range(height + 1):
        row = lines[2 * r]
        for c in range(width):
            seg = row[3 * c + 1 : 3 * c + 3]
            if 1 <= r <= height - 1:
                _apply_edge(edge_map[(r - 1, c, r, c)], seg, cell_map, fillable, r - 1, c, r, c)
        # Watchtowers (radar) at EVERY wall corner (0..=height × 0..=width),
        # including the outer border.  Corner char sits at 3*c (game stride);
        # rows with a leading indent read ' ' at grid columns outside the
        # playable shape (blocked), so those corners simply carry no radar.
        for c in range(width + 1):
            ch = row[3 * c] if 3 * c < len(row) else " "
            if ch.isdigit():
                cell_map.setdefault("__vertices", {})
                cell_map["__vertices"][(r, c)] = int(ch)


def _collect_edges(edge_map: dict, fillable: list) -> list[dict]:
    edges: list[dict] = []
    for e in edge_map.values():
        if e.get("is_boundary") or e.get("constraint") is not None:
            edges.append(e)
        else:
            r1, c1, r2, c2 = e["r1"], e["c1"], e["r2"], e["c2"]
            # gap (no glyph) between two fillable cells => forced boundary
            if fillable[r1][c1] and fillable[r2][c2] and not _has_glyph(e):
                e["is_boundary"] = True
                edges.append(e)
    return edges


def _build_vertices(cell_map: dict) -> list[dict]:
    vt = cell_map.get("__vertices", {})
    return [{"row": vr, "col": vc, "watchtower": n} for (vr, vc), n in sorted(vt.items())]


def _build_outer_boundaries(width: int, height: int) -> list[dict]:
    outer_boundaries: list[dict] = []
    for c in range(width):
        outer_boundaries.append({"r1": 0, "c1": c, "r2": 0, "c2": c + 1})
        outer_boundaries.append({"r1": height, "c1": c, "r2": height, "c2": c + 1})
    for r in range(height):
        outer_boundaries.append({"r1": r, "c1": 0, "r2": r + 1, "c2": 0})
        outer_boundaries.append({"r1": r, "c1": width, "r2": r + 1, "c2": width})
    return outer_boundaries


def _blocked_set(fillable: list) -> set[tuple[int, int]]:
    height = len(fillable)
    width = len(fillable[0]) if height else 0
    return {(r, c) for r in range(height) for c in range(width) if not fillable[r][c]}


def _init_grid(height: int, width: int):
    """Build the empty cells / cell_map / fillable / edge_map scaffolding."""
    cells: list[dict] = []
    for r in range(height):
        for c in range(width):
            cells.append({"row": r, "col": c})

    cell_map = {(c["row"], c["col"]): c for c in cells}
    fillable = [[True] * width for _ in range(height)]

    edge_map: dict[tuple[int, int, int, int], dict] = {}
    for r in range(height):
        for c in range(width - 1):
            edge_map[(r, c, r, c + 1)] = {"r1": r, "c1": c, "r2": r, "c2": c + 1}
    for r in range(height - 1):
        for c in range(width):
            edge_map[(r, c, r + 1, c)] = {"r1": r, "c1": c, "r2": r + 1, "c2": c}

    return cells, cell_map, fillable, edge_map


def _parse_puzzle(p: dict) -> dict:
    width, height = int(p["width"]), int(p["height"])
    lines = [ln.ljust(3 * width + 1, " ") for ln in p.get("puzzle_grid", [])]
    if len(lines) < 2 * height + 1:
        lines.extend([" " * (3 * width + 1)] * (2 * height + 1 - len(lines)))

    shapes = parse_shapes(p)
    shape_ids = set(shapes)
    is_spr = bool(p.get("one_symbol_per_region"))
    has_shape_bank = bool(p.get("shape_bank"))

    cells, cell_map, fillable, edge_map = _init_grid(height, width)

    flags = {"compass": False, "fence": False, "numbers": False, "shape_pattern": False}
    p_symbols: dict[str, int] = {}
    p_symbol_positions: dict[tuple[int, int], str] = {}

    _parse_puzzle_cell_rows(
        lines,
        height,
        width,
        shape_ids,
        is_spr,
        shapes,
        cell_map,
        fillable,
        edge_map,
        p_symbols,
        p_symbol_positions,
        flags,
    )
    _parse_puzzle_wall_rows(lines, height, width, edge_map, fillable, cell_map)

    edges = _collect_edges(edge_map, fillable)
    constraint_types = {e["constraint"]["type"] for e in edges if e.get("constraint")}
    vertices = _build_vertices(cell_map)
    outer_boundaries = _build_outer_boundaries(width, height)

    has_compass = flags["compass"]
    has_fence = flags["fence"]
    has_numbers = flags["numbers"]
    has_shape_pattern = flags["shape_pattern"]

    rules = build_rules(
        p,
        shapes,
        has_compass,
        has_fence,
        has_numbers,
        constraint_types,
        bool(vertices),
        is_spr,
        has_shape_bank,
        has_shape_pattern,
        p_symbols,
        rose_types=_is_rose_window(
            p,
            p_symbols,
            _blocked_set(fillable),
            sym_pos=p_symbol_positions,
        ),
    )

    return {
        "version": "1.0",
        "grid": {"height": height, "width": width},
        "cells": cells,
        "edges": edges,
        "vertices": vertices,
        "outer_boundaries": outer_boundaries,
        "rules": rules,
        "shape_pool": [_shape_list(s) for s in shapes.values()],
        "_meta": {
            "archive_id": p.get("id"),
            "archive_type": p.get("type"),
            "archive_difficulty": p.get("difficulty"),
        },
    }


def _has_glyph(e: dict) -> bool:
    return e.get("_glyph", False)


def _apply_edge(
    entry: dict,
    glyph: str,
    _cell_map: dict,
    _fillable: list,
    _r1: int,
    _c1: int,
    _r2: int,
    _c2: int,
) -> None:
    entry["_glyph"] = glyph not in ("  ", " ")
    rule = _EDGE_GLYPH_RULES.get(glyph)
    if rule is not None:
        ctype, value = rule
        if ctype is not None:
            if value is not None:
                entry["constraint"] = {"type": ctype, "value": value}
            else:
                entry["constraint"] = {"type": ctype}
        entry["is_boundary"] = True
        return
    if re.fullmatch(r"-\d", glyph):
        entry["constraint"] = {"type": "difference", "value": int(glyph[1])}
        entry["is_boundary"] = True
        return
    if glyph.isdigit():
        entry["constraint"] = {"type": "difference", "value": int(glyph)}
        entry["is_boundary"] = True
        return
    # glyph in ("--","|") or a space, or any other unknown wall glyph: no-op


def _shape_bank_rule(p: dict, shapes: dict[int, Shape]) -> dict:
    bank = p["shape_bank"]
    pool = [shapes[i] for i in bank if i in shapes] or list(shapes.values())
    return {"type": "shape_pool", "params": {"shapes": [_shape_list(s) for s in pool]}}


def _add_flag_rules(p: dict, rules: list[dict]) -> None:
    for key, rule_type in _FLAG_RULES:
        if p.get(key):
            rules.append({"type": rule_type})


def _add_area_rule(p: dict, rules: list[dict]) -> None:
    if p.get("area_equals") is not None:
        rules.append({"type": "precise", "params": {"area": int(p["area_equals"])}})
        return
    params: dict = {}
    if p.get("area_at_least") is not None:
        params["min"] = int(p["area_at_least"])
    if p.get("area_at_most") is not None:
        params["max"] = int(p["area_at_most"])
    if params:
        rules.append({"type": "range", "params": params})


def build_rules(
    p: dict,
    shapes: dict[int, Shape],
    has_compass: bool,
    has_fence: bool,
    has_numbers: bool,
    constraint_types: set[str],
    has_watchtower: bool,
    _is_spr: bool,
    has_shape_bank: bool,
    has_shape_pattern: bool,
    _p_symbols: dict[str, int] | None = None,
    rose_types: list[str] | None = None,
) -> list[dict]:
    rules: list[dict] = []

    if has_shape_bank and shapes:
        rules.append(_shape_bank_rule(p, shapes))

    if has_shape_pattern:
        rules.append({"type": "puzzle_piece"})

    _add_flag_rules(p, rules)
    _add_area_rule(p, rules)

    # Boolean flags that map directly to a single rule type.
    simple_rules = [
        (has_numbers, "area"),
        (has_fence, "fence"),
        (has_compass, "compass"),
        (bool(p.get("one_symbol_per_region")), "solitary"),
        ("inequality" in constraint_types, "inequality"),
        ("difference" in constraint_types, "difference"),
        ("homogeneous" in constraint_types, "homogeneous"),
        ("heterogeneous" in constraint_types, "heterogeneous"),
        (has_watchtower, "watchtower"),
    ]
    for flag, rule_type in simple_rules:
        if flag:
            rules.append({"type": rule_type})

    # Rose window: the P-number coloured circles. Verified against the
    # archive's official solution (N types x M occurrences, M regions each
    # containing all N types) — see _is_rose_window.
    if rose_types:
        rules.append(
            {
                "type": "rose_window",
                "params": {"symbol_types": rose_types},
            }
        )

    return rules


# ---------------------------------------------------------------------------
# Solution-region validation
# ---------------------------------------------------------------------------
def _region_is_connected(cells: frozenset) -> bool:
    if not cells:
        return False
    seen: set[tuple[int, int]] = set()
    stack = [next(iter(cells))]
    while stack:
        r, c = stack.pop()
        if (r, c) in seen:
            continue
        seen.add((r, c))
        for dr, dc in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            n = (r + dr, c + dc)
            if n in cells and n not in seen:
                stack.append(n)
    return seen == set(cells)


def _region_connectivity_error(cells: frozenset) -> str | None:
    if not cells:
        return "official solution contains an empty region"
    if not _region_is_connected(cells):
        return "official solution contains a disconnected region"
    return None


def _validate_solution_partition(
    part: list, height: int, width: int, blocked: set[tuple[int, int]]
) -> None:
    """Raise ValueError unless *part* covers fillable cells with connected regions."""
    fillable = {(r, c) for r in range(height) for c in range(width) if (r, c) not in blocked}
    all_cells: set[tuple[int, int]] = set()
    for cells in part:
        all_cells.update(cells)
    if all_cells != fillable:
        raise ValueError(
            "official solution does not cover exactly the fillable cells: "
            f"missing={sorted(fillable - all_cells)[:5]} "
            f"extra={sorted(all_cells - fillable)[:5]}"
        )

    for cells in part:
        err = _region_connectivity_error(cells)
        if err is not None:
            raise ValueError(err)


def archive_solution_regions(p: dict) -> list[list[tuple[int, int]]]:
    """Decode the archive's official ``solution`` into a region partition.

    Returns one list of ``(row, col)`` per region, or ``[]`` when the archive
    has no official solution for this puzzle (e.g. 0067 / 1130).  The partition
    is validated: it must cover exactly the fillable cells, each region must be
    4-connected, and no region may contain a blocked cell.
    """
    sol = p.get("solution")
    if not sol:
        return []
    out = _parse_puzzle(p)
    blocked = {(c["row"], c["col"]) for c in out["cells"] if c.get("blocked")}
    height, width = out["grid"]["height"], out["grid"]["width"]
    part = _archive_partition(p, blocked)
    _validate_solution_partition(part, height, width, blocked)
    return [sorted(cells) for cells in part]


# ---------------------------------------------------------------------------
# Top-level conversion
# ---------------------------------------------------------------------------
def _clear_dir(zone_dir: str) -> None:
    for name in os.listdir(zone_dir):
        path = os.path.join(zone_dir, name)
        if os.path.isfile(path):
            os.remove(path)
        elif os.path.isdir(path):
            for fname in os.listdir(path):
                os.remove(os.path.join(path, fname))
            os.rmdir(path)


def _clear_output_root() -> None:
    for zone in ("Zone1", "Zone2", "Zone3"):
        zone_dir = os.path.join(OUT_ROOT, zone)
        if os.path.isdir(zone_dir):
            _clear_dir(zone_dir)


def _convert_one(p: dict, dry_run: bool) -> tuple[str, object] | None:
    """Convert a single puzzle dict.

    Returns ``None`` (skip), or a tuple ``(kind, payload)`` where kind is
    ``"error"`` / ``"dry"`` / ``"written"``.
    """
    zone = p.get("zone")
    if zone not in ("Zone1", "Zone2", "Zone3"):
        return None
    ptype = p.get("type", "misc")
    pid = p.get("id", "unknown")
    try:
        out = _parse_puzzle(p)
        dict_to_puzzle(out)  # round-trip validation
    except Exception as e:  # noqa: BLE001
        return ("error", f"{zone}/{ptype}/{pid}: {e}")
    if dry_run:
        return ("dry", (zone, ptype))
    out_dir = os.path.join(OUT_ROOT, zone, ptype)
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, f"{pid}.json"), "w", encoding="utf-8") as f:
        json.dump(out, f, ensure_ascii=False, indent=2)
    return ("written", (zone, ptype))


def convert_all(dry_run: bool = False) -> tuple[int, list[str]]:
    with open(ARCHIVE_PATH, encoding="utf-8") as f:
        data = json.load(f)

    if not dry_run:
        _clear_output_root()

    written = 0
    errors: list[str] = []
    per_zone: dict[str, dict[str, int]] = {}
    for p in data:
        res = _convert_one(p, dry_run)
        if res is None:
            continue
        kind, payload = res
        if kind == "error":
            errors.append(payload)  # type: ignore[arg-type]
            continue
        written += 1
        zone, ptype = payload  # type: ignore[misc]
        per_zone.setdefault(zone, {}).setdefault(ptype, 0)
        per_zone[zone][ptype] += 1

    if dry_run:
        for zone, types in per_zone.items():
            print(f"{zone}: {sum(types.values())} puzzles in {len(types)} types")
    return written, errors


def main() -> None:
    dry_run = "--dry-run" in sys.argv
    written, errors = convert_all(dry_run=dry_run)
    print(f"{'Dry-run' if dry_run else 'Converted'} {written} puzzles")
    if errors:
        print(f"Errors ({len(errors)}):")
        for e in errors:
            print("  " + e)
        sys.exit(1)


if __name__ == "__main__":
    main()
