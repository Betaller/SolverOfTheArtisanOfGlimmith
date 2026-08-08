#!/usr/bin/env python3
"""Parser for the official puzzle corpus of "The Artisan of Glimmith".

Reads ``third_party/archiveofglimmith.github.io/puzzles.json`` and produces
structured region / clue data suitable for statistical analysis.

Geometry convention
-------------------
Every vertex-row line (even index ``2*r``) and cell-row line (odd index
``2*r+1``) uses a character grid where vertex / vertical-edge positions are at
character columns that are multiples of **3**.  For a cell at board coordinate
``(r, c)``:

* Its four corner vertices sit at character columns ``3*c`` and ``3*(c+1)``
  in vertex rows ``2*r`` and ``2*(r+1)``.
* Its content occupies character columns ``3*c+1`` and ``3*c+2`` in the cell
  row ``2*r+1`` (two characters for ordinary clues).
* Vertical edges (``|`` or a clue char like ``=``, ``!``, ``>``, ``<``, ``#``)
  are at character column ``3*(c+1)`` in the cell row — they replace the ``|``
  that would normally separate cell ``c`` from cell ``c+1``.

Variable-length clues (compass ``U1DLR``, shape ``S10``, …) expand the cell-row
*text* beyond the fixed vertex-row width, causing ``|`` positions to shift away
from multiples of 3.  The vertex rows and the solution always keep the standard
3-char-per-cell geometry.  Cell-row tokenisation therefore uses a hybrid
strategy:

* **Fixed-geometry rows** (all ``|`` at multiples of 3): read cell content
  directly from positions ``3*c+1`` … ``3*c+2``; read v-edge clues from
  position ``3*(c+1)``.
* **Variable-geometry rows** (some ``|`` not at multiples of 3): split on ``|``
  to obtain tokens, then split each token on v-edge clue characters
  (``= ! > < # ^ v`` — never part of cell content) to separate merged cells
  from their inter-cell v-edge clues.

Digit v-edge clues (``4-difference`` puzzles) only appear in fixed-geometry
rows, so the geometric read handles them correctly.
"""

from __future__ import annotations

import json
import os
import re
from typing import Any

CORPUS_PATH = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "third_party",
    "archiveofglimmith.github.io",
    "puzzles.json",
)

# Characters that are "framework" (not clues) at edge / vertex positions.
_H_EDGE_FRAMEWORK = frozenset("+- ")
_V_EDGE_FRAMEWORK = frozenset("| ")
_VERTEX_FRAMEWORK = frozenset("+ ")

# Characters that constitute a wall in the solution.
_WALL_CHARS = frozenset("#+-|")

# V-edge clue characters that can appear *inside* a cell-row token (replacing ``|``).
# These never appear inside cell *content*, so splitting on them is safe in
# variable-geometry rows.  Digits are excluded because they can be part of cell
# content (area numbers, compass counts); digit v-edges are handled by the
# fixed-geometry geometric read.
_V_EDGE_SPLIT_CHARS = frozenset("=!><#^v")

# Compiled regex for splitting a token on v-edge clue characters.  Capturing
# group keeps the delimiter so we can record it as a clue.
_V_EDGE_SPLIT_RE = re.compile(r"([=!><#^v])")


# ---------------------------------------------------------------------------
# Low-level geometry helpers
# ---------------------------------------------------------------------------
def _vertex_cols(vtx_line: str) -> list[int]:
    """Return sorted column indices ``c`` where a ``+`` vertex exists in *vtx_line*."""
    return [j // 3 for j, ch in enumerate(vtx_line) if ch == "+" and j % 3 == 0]


def _h_edge_chars(vtx_line: str, c: int) -> str:
    """Return the raw characters of the horizontal edge above cell column *c*.

    The edge sits between vertex ``3*c`` and ``3*(c+1)`` — character columns
    ``3*c+1`` through ``3*(c+1)-1`` (inclusive).
    """
    start = 3 * c + 1
    end = 3 * (c + 1)
    if end > len(vtx_line):
        return ""
    return vtx_line[start:end]


def _h_edge_present(vtx_line: str, c: int) -> bool:
    """True iff the horizontal edge above column *c* is drawn (non-space)."""
    return _h_edge_chars(vtx_line, c).strip() != ""


def _is_fixed_geometry(cell_line: str, vtx_line: str) -> bool:
    """True iff the cell row uses standard 2-char-per-cell geometry.

    This holds iff, at every vertex position ``3*c`` where the vertex row above
    has a ``+``, the cell row has a delimiter character (``|``, a v-edge clue
    char, or a space for board gaps) rather than cell-content text bleeding
    across the boundary.  When variable-length clues (compass, shape markers)
    expand a cell, the ``|`` that would sit at ``3*c`` is pushed rightward,
    causing cell-content letters to appear at what should be a vertex position.
    """
    for j, ch in enumerate(vtx_line):
        if ch == "+" and j % 3 == 0:
            if j >= len(cell_line):
                return False
            cell_ch = cell_line[j]
            if cell_ch == " ":
                continue  # board gap — fine for irregular boards
            if cell_ch == "|":
                continue
            # V-edge clue chars (including digits for difference puzzles).
            if cell_ch in _V_EDGE_SPLIT_CHARS or cell_ch.isdigit():
                continue
            # Cell-content letter at a vertex position → variable geometry.
            return False
    return True


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------
def load_corpus() -> list[dict]:
    """Load and return the full puzzle corpus as a list of dicts."""
    with open(CORPUS_PATH, encoding="utf-8") as f:
        return json.load(f)


def _row_candidates(grid: list[str], r: int) -> list[int]:
    """Candidate columns for row *r* from vertex rows (both top and bottom h-edges present)."""
    top_idx = 2 * r
    bot_idx = 2 * r + 2
    if top_idx >= len(grid):
        return []
    top_line = grid[top_idx]
    bot_line = grid[bot_idx] if bot_idx < len(grid) else ""
    top_verts = set(_vertex_cols(top_line))
    bot_verts = set(_vertex_cols(bot_line))
    candidates: list[int] = []
    for c in sorted(top_verts):
        if (c + 1) not in top_verts:
            continue
        if not _h_edge_present(top_line, c):
            continue
        if c not in bot_verts or (c + 1) not in bot_verts:
            continue
        if not _h_edge_present(bot_line, c):
            continue
        candidates.append(c)
    return candidates


def get_real_cells(puzzle: dict) -> set[tuple[int, int]]:
    """Return the set of ``(row, col)`` cells that actually exist on the board.

    A cell ``(r, c)`` exists iff:

    1. Both the horizontal edge above (vertex row ``2*r``) and below (vertex row
       ``2*(r+1)``) are drawn.
    2. The cell row ``2*r+1`` has non-space content at the cell's position —
       i.e. the cell is not a board-exterior gap masquerading as a real cell
       between two drawn edges (this happens on disconnected / perforated
       boards).

    For fixed-geometry rows (standard 2-char cells), condition 2 is checked
    geometrically at character positions ``3*c+1`` … ``3*c+2``.  For
    variable-geometry rows (compass / shape clues that expand the text), the
    cell-row tokens are split on ``|`` and v-edge clue characters; gap tokens
    (all spaces) represent missing cells whose count is inferred from the gap
    width, and the remaining non-gap tokens are mapped to candidate columns in
    left-to-right order.
    """
    grid: list[str] = puzzle.get("puzzle_grid", [])
    height: int = puzzle.get("height", 0)
    cells: set[tuple[int, int]] = set()

    for r in range(height):
        candidates = _row_candidates(grid, r)
        if not candidates:
            continue
        cell_idx = 2 * r + 1
        if cell_idx >= len(grid):
            continue
        cell_line = grid[cell_idx]
        vtx_line = grid[2 * r] if (2 * r) < len(grid) else ""

        if _is_fixed_geometry(cell_line, vtx_line):
            # Fixed geometry: check cell content at 3*c+1 .. 3*c+2.
            for c in candidates:
                start = 3 * c + 1
                end = 3 * c + 3
                content = cell_line[start:end] if end <= len(cell_line) else "  "
                if content.strip() != "":
                    cells.add((r, c))
        else:
            # Variable geometry: split on | then on v-edge chars, map to candidates.
            parts = cell_line.split("|")
            segments = parts[1:-1] if len(parts) > 2 else []
            # Build a list of (is_cell, content_or_gap_width) by splitting on v-edge chars.
            # Each non-gap piece is a cell; each gap piece represents missing cells.
            pieces: list[tuple[bool, str]] = []  # (is_cell, content)
            for seg in segments:
                if seg.strip() == "":
                    pieces.append((False, seg))
                else:
                    sub = _V_EDGE_SPLIT_RE.split(seg)
                    for piece in sub:
                        if piece == "":
                            continue
                        if len(piece) == 1 and piece in _V_EDGE_SPLIT_CHARS:
                            # V-edge delimiter — not a cell, skip.
                            continue
                        pieces.append((True, piece.strip()))

            # Count non-cell (gap) pieces' missing-cell count and verify total.
            non_cell_total = 0
            cell_total = 0
            for is_cell, val in pieces:
                if is_cell:
                    cell_total += 1
                else:
                    non_cell_total += (len(val) + 1) // 3

            if cell_total + non_cell_total == len(candidates):
                # Map: non-cell pieces skip candidates, cell pieces consume them.
                i = 0
                for is_cell, val in pieces:
                    if is_cell:
                        if i < len(candidates):
                            cells.add((r, candidates[i]))
                        i += 1
                    else:
                        i += (len(val) + 1) // 3
            elif cell_total == len(candidates):
                # No gaps detected — map 1:1.
                for k in range(min(cell_total, len(candidates))):
                    cells.add((r, candidates[k]))
            else:
                # Fallback: use fixed-geometry geometric check.
                for c in candidates:
                    start = 3 * c + 1
                    end = 3 * c + 3
                    content = cell_line[start:end] if end <= len(cell_line) else "  "
                    if content.strip() != "":
                        cells.add((r, c))

    return cells


def parse_solution_regions(puzzle: dict) -> list[frozenset[tuple[int, int]]]:
    """Parse the solution into a list of regions (each a frozenset of ``(r, c)``).

    Uses union-find: two orthogonally-adjacent real cells belong to the same
    region iff the edge between them in the solution is **not** a wall (a wall
    is any non-space character: ``#``, ``+``, ``-``, ``|``).

    The solution always uses fixed 3-char-per-cell geometry (walls ``+``, ``|``,
    ``#`` at multiples of 3), even for puzzles whose puzzle_grid has
    variable-width cell rows.
    """
    real_cells = get_real_cells(puzzle)
    sol: list[str] = puzzle.get("solution", [])
    if not sol:
        return []

    parent: dict[tuple[int, int], tuple[int, int]] = {cell: cell for cell in real_cells}

    def find(x: tuple[int, int]) -> tuple[int, int]:
        root = x
        while parent[root] != root:
            root = parent[root]
        while parent[x] != root:
            parent[x], x = root, parent[x]
        return root

    def union(a: tuple[int, int], b: tuple[int, int]) -> None:
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[ra] = rb

    cell_grid = real_cells

    for (r, c) in real_cells:
        # Horizontal neighbour (r, c+1): check vertical edge in solution cell row.
        if (r, c + 1) in cell_grid:
            cell_row_idx = 2 * r + 1
            if cell_row_idx < len(sol):
                pos = 3 * (c + 1)
                ch = sol[cell_row_idx][pos] if pos < len(sol[cell_row_idx]) else " "
                if ch not in _WALL_CHARS:
                    union((r, c), (r, c + 1))
        # Vertical neighbour (r+1, c): check horizontal edge in solution vertex row.
        if (r + 1, c) in cell_grid:
            vtx_row_idx = 2 * (r + 1)
            if vtx_row_idx < len(sol):
                edge_str = _h_edge_chars(sol[vtx_row_idx], c)
                if edge_str.strip() == "":  # all spaces -> open -> same region
                    union((r, c), (r + 1, c))

    groups: dict[tuple[int, int], set[tuple[int, int]]] = {}
    for cell in real_cells:
        root = find(cell)
        groups.setdefault(root, set()).add(cell)

    return [frozenset(g) for g in groups.values()]


def _tokenize_cell_row(
    cell_line: str, vtx_line: str, row_cols: list[int], r: int, width: int
) -> tuple[dict[tuple[int, int], str], dict[tuple[int, int], str]]:
    """Parse one cell row into cell clues and v-edge clues.

    Parameters
    ----------
    cell_line
        The cell-row text (puzzle_grid line ``2*r+1``).
    vtx_line
        The vertex-row text above (puzzle_grid line ``2*r``).
    row_cols
        Sorted list of real column indices for this row.
    r
        Row index (for building result keys).
    width
        Declared puzzle width (for fixed-geometry column iteration).

    Returns
    -------
    (cell_clues, v_edge_clues)
        ``cell_clues`` maps ``(r, c)`` -> clue string (only for non-empty cells).
        ``v_edge_clues`` maps ``(r, c)`` -> single clue char for the edge to
        the *right* of cell ``(r, c)`` (i.e. between columns ``c`` and ``c+1``).
    """
    cell_clues: dict[tuple[int, int], str] = {}
    v_edge_clues: dict[tuple[int, int], str] = {}

    if not row_cols:
        return cell_clues, v_edge_clues

    fixed = _is_fixed_geometry(cell_line, vtx_line)

    if fixed:
        # Fixed geometry: read cell content and v-edge clues by position.
        for c in row_cols:
            # Cell content at 3*c+1 .. 3*c+2.
            start = 3 * c + 1
            end = 3 * c + 3
            content = cell_line[start:end].strip() if end <= len(cell_line) else ""
            if content and content != "..":
                cell_clues[(r, c)] = content
            # V-edge to the right of this cell at position 3*(c+1).
            vpos = 3 * (c + 1)
            if vpos < len(cell_line):
                vch = cell_line[vpos]
                if vch not in _V_EDGE_FRAMEWORK:
                    v_edge_clues[(r, c)] = vch
    else:
        # Variable geometry: split on ``|`` then on v-edge clue chars.
        parts = cell_line.split("|")
        # Inner parts (excluding leading/trailing board-gap fragments).
        raw_tokens = parts[1:-1] if len(parts) > 2 else []

        # Further split each raw token on v-edge clue characters and collect
        # (cell_content, v_edge_char_or_None) pairs in left-to-right order.
        cell_contents: list[str] = []
        v_edges: list[str | None] = []  # v_edge char after this cell, or None

        for tok in raw_tokens:
            # Skip board-gap tokens (all spaces) — they don't correspond to cells.
            if tok.strip() == "":
                continue
            # Split on v-edge clue chars, keeping the delimiters.
            sub = _V_EDGE_SPLIT_RE.split(tok)
            # sub is like ['..', '#', 'S5'] or ['U1DLR'] or ['..', '#', '..', '#', '..']
            i = 0
            while i < len(sub):
                piece = sub[i]
                if piece == "":
                    i += 1
                    continue
                # If this piece is a single v-edge char, it's a delimiter (shouldn't
                # happen as first piece, but handle gracefully).
                if len(piece) == 1 and piece in _V_EDGE_SPLIT_CHARS:
                    # This v-edge belongs after the previous cell.
                    if cell_contents:
                        v_edges[-1] = piece
                    i += 1
                    continue
                # Otherwise it's a cell content token.
                content = piece.strip()
                cell_contents.append(content)
                v_edges.append(None)
                i += 1
                # Check if next piece is a v-edge delimiter.
                if i < len(sub) and len(sub[i]) == 1 and sub[i] in _V_EDGE_SPLIT_CHARS:
                    v_edges[-1] = sub[i]
                    i += 1

        # Map cell_contents to real columns in order.
        n = min(len(cell_contents), len(row_cols))
        for k in range(n):
            c = row_cols[k]
            content = cell_contents[k]
            if content and content != "..":
                cell_clues[(r, c)] = content
            # Record v-edge clue if present and the next column is also real.
            if k < len(v_edges) and v_edges[k] is not None:
                if (k + 1) < len(row_cols):
                    v_edge_clues[(r, c)] = v_edges[k]

    return cell_clues, v_edge_clues


def parse_puzzle_clues(puzzle: dict) -> dict[str, Any]:
    """Parse all clues from the puzzle grid.

    Returns a dict with keys:
        ``cell``    — ``{(r, c): clue_string}``  (e.g. ``"04"``, ``"F3"``, ``"U1DLR"``)
        ``h_edge``  — ``{(r, c): char}``          (horizontal edge between row r-1 and r)
        ``v_edge``  — ``{(r, c): char}``          (vertical edge between col c and c+1, at row r)
        ``vertex``  — ``{(r, c): char}``          (vertex at grid corner (r, c))

    Only edges / vertices adjacent to at least one real cell are included.
    Cells whose content is ``".."`` are not included in ``cell``.
    """
    grid: list[str] = puzzle.get("puzzle_grid", [])
    height: int = puzzle.get("height", 0)
    width: int = puzzle.get("width", 0)
    real_cells = get_real_cells(puzzle)

    result: dict[str, Any] = {"cell": {}, "h_edge": {}, "v_edge": {}, "vertex": {}}

    # --- Cell clues + v-edge clues (from cell rows) -------------------------
    for r in range(height):
        cell_idx = 2 * r + 1
        if cell_idx >= len(grid):
            break
        row_cols = sorted(c for (rr, c) in real_cells if rr == r)
        if not row_cols:
            continue
        cell_line = grid[cell_idx]
        vtx_line = grid[2 * r] if (2 * r) < len(grid) else ""
        cell_clues, v_edge_clues = _tokenize_cell_row(cell_line, vtx_line, row_cols, r, width)
        result["cell"].update(cell_clues)
        result["v_edge"].update(v_edge_clues)

    # --- Horizontal-edge clues (from vertex rows) ---------------------------
    for r in range(height + 1):
        vtx_idx = 2 * r
        if vtx_idx >= len(grid):
            break
        vtx_line = grid[vtx_idx]
        verts = set(_vertex_cols(vtx_line))
        for c in sorted(verts):
            if (c + 1) not in verts:
                continue
            edge_str = _h_edge_chars(vtx_line, c)
            clue_chars = [ch for ch in edge_str if ch not in _H_EDGE_FRAMEWORK]
            if clue_chars:
                clue = clue_chars[0]
                above = (r - 1, c)
                below = (r, c)
                if above in real_cells or below in real_cells:
                    result["h_edge"][(r, c)] = clue

    # --- Vertex clues -------------------------------------------------------
    for r in range(height + 1):
        vtx_idx = 2 * r
        if vtx_idx >= len(grid):
            break
        vtx_line = grid[vtx_idx]
        for c in _vertex_cols(vtx_line):
            pos = 3 * c
            if pos >= len(vtx_line):
                continue
            ch = vtx_line[pos]
            if ch not in _VERTEX_FRAMEWORK:
                adjacent = any(
                    (rr, cc) in real_cells
                    for rr in (r - 1, r)
                    for cc in (c - 1, c)
                )
                if adjacent:
                    result["vertex"][(r, c)] = ch

    return result


# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------
def _self_test() -> None:
    corpus = load_corpus()
    print(f"Loaded {len(corpus)} puzzles.\n")

    # --- Coverage check -----------------------------------------------------
    ok_count = 0
    fail_count = 0
    failures: list[str] = []
    empty_count = 0

    for p in corpus:
        sol = p.get("solution", [])
        if not sol or all(not line.strip() for line in sol):
            empty_count += 1
            continue
        real_cells = get_real_cells(p)
        regions = parse_solution_regions(p)
        total_covered = sum(len(reg) for reg in regions)
        if total_covered == len(real_cells):
            ok_count += 1
        else:
            fail_count += 1
            failures.append(
                f"  {p['id']:>8s}  type={p['type']:<20s}  "
                f"real={len(real_cells)}  covered={total_covered}  "
                f"regions={len(regions)}"
            )

    print("=== Coverage Check ===")
    print(f"  OK (full coverage):   {ok_count}")
    print(f"  FAIL (partial):       {fail_count}")
    print(f"  Empty solution:       {empty_count}")
    if failures:
        print("\nFailures:")
        for f in failures:
            print(f)
    print()

    # --- Sample parses ------------------------------------------------------
    sample_ids = ["0008", "0162", "0445", "0299", "0187", "0152"]
    by_id = {p["id"]: p for p in corpus}

    print("=== Sample Parses ===\n")
    for sid in sample_ids:
        p = by_id.get(sid)
        if p is None:
            print(f"--- id={sid}: NOT FOUND ---\n")
            continue
        real_cells = get_real_cells(p)
        regions = parse_solution_regions(p)
        clues = parse_puzzle_clues(p)
        region_sizes = sorted((len(reg) for reg in regions), reverse=True)

        print(f"--- id={sid}  type={p['type']}  {p['width']}x{p['height']} ---")
        print(f"  Real cells:      {len(real_cells)}")
        print(f"  Region count:    {len(regions)}")
        print(f"  Region sizes:    {region_sizes}")
        cell_clues = clues["cell"]
        if cell_clues:
            print(f"  Cell clues ({len(cell_clues)}):")
            for (r, c), clue in sorted(cell_clues.items()):
                print(f"    ({r},{c}): {clue}")
        h_edges = clues["h_edge"]
        if h_edges:
            print(f"  H-edge clues ({len(h_edges)}):")
            for (r, c), clue in sorted(h_edges.items()):
                print(f"    ({r},{c}): {clue}")
        v_edges = clues["v_edge"]
        if v_edges:
            print(f"  V-edge clues ({len(v_edges)}):")
            for (r, c), clue in sorted(v_edges.items()):
                print(f"    ({r},{c}): {clue}")
        vertex_clues = clues["vertex"]
        if vertex_clues:
            print(f"  Vertex clues ({len(vertex_clues)}):")
            for (r, c), clue in sorted(vertex_clues.items()):
                print(f"    ({r},{c}): {clue}")
        if not any([cell_clues, h_edges, v_edges, vertex_clues]):
            print("  (no clues)")
        print()


if __name__ == "__main__":
    _self_test()
