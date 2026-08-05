"""Convert official puzzles from the archive (puzzles.json) to C++ AoG_Solver .puz files.

The archive (third_party/archiveofglimmith.github.io/puzzles.json) stores every official
puzzle in the game's native ASCII layout: ``puzzle_grid`` (the puzzle) and ``solution``
(its official region-boundary answer).  The C++ solver in third_party/AoG_Solver reads
``.puz`` files whose ``PUZZLE`` / ``SOLUTION`` sections are exactly those grids, with each
line padded to ``3*width+1`` characters — the archive trims trailing whitespace, the game
and its ``.puz`` files keep it.

This script rebuilds the ``aog_puzzles/`` directory that the C++ solver's ``batch_run.sh``
needs.  It lives in the main repo (not inside the third_party submodule) so the .puz files
are tracked by the project:

    aog_puzzles/<zone>/<type>/<id>.puz

The header directives consumed by the C++ ``main()`` are derived straight from the archive
metadata:

  * ``shapes`` / ``shape_bank``  -> ``SHAPE`` definitions + ``SHAPE_BANK <n>``
  * rule flags                  -> ``ADJACENT_SHAPES_DIFFERENT`` / ``ALL_SHAPES_SAME`` / ...
  * ``area_equals`` / ``area_at_least`` / ``area_at_most``
                                -> ``AREA_EQUALS`` / ``AREA_AT_LEAST`` / ``AREA_AT_MOST``

The grid itself is passed through verbatim (only trailing-padded), never re-encoded:
compass cells (``UDLR``…), fence (``F0``-``F7``), radar digits at vertices, slash packs
(``P1``-``P4``), difference counts, inequality symbols, and pre-drawn ``##`` boundaries all
already use the byte-level encoding the C++ parser expects.

Output: aog_puzzles/{zone}/{type}/{id}.puz
Usage:
    python scripts/convert_puzzles_json_to_aog.py [--out aog_puzzles]
    python scripts/convert_puzzles_json_to_aog.py --ids 0008 0079   # debug a subset
"""
from __future__ import annotations

import argparse
import json
import os

ARCHIVE_JSON = os.path.join("third_party", "archiveofglimmith.github.io", "puzzles.json")

# Archive boolean flag -> C++ header directive.
RULE_FLAGS: list[tuple[str, str]] = [
    ("adjacent_shapes_different", "ADJACENT_SHAPES_DIFFERENT"),
    ("adjacent_sizes_different", "ADJACENT_SIZES_DIFFERENT"),
    ("all_shapes_different", "ALL_SHAPES_DIFFERENT"),
    ("all_shapes_same", "ALL_SHAPES_SAME"),
    ("no_3_way_intersections", "NO_3_WAY_INTERSECTIONS"),
    ("no_4_way_intersections", "NO_4_WAY_INTERSECTIONS"),
    ("no_rectangles", "NO_RECTANGLES"),
    ("one_symbol_per_region", "ONE_SYMBOL_PER_REGION"),
    ("only_rectangles", "ONLY_RECTANGLES"),
]

# Archive area-bound field -> C++ directive taking a value.
AREA_BOUNDS: list[tuple[str, str]] = [
    ("area_equals", "AREA_EQUALS"),
    ("area_at_least", "AREA_AT_LEAST"),
    ("area_at_most", "AREA_AT_MOST"),
]


def _compass_end(j: int, line: str) -> int:
    """Replicate the C++ compass-cell parsing to find where the cell ends.

    The C++ read_puzzle() walks the four directions starting at the 'U', advancing
    a cursor per direction; chars past the line end read as '\\0'.  This mirrors that
    exactly so the returned index matches what the solver computes.
    """
    n = len(line)

    def g(i: int) -> str:
        return line[i] if i < n else "\0"

    ci = j
    # up: marker 'D'
    if g(ci + 1) != "D" and g(ci + 2) != "D":
        ci += 2
    elif g(ci + 1) != "D":
        ci += 1
    ci += 1
    # down: marker 'L'
    if g(ci + 1) != "L" and g(ci + 2) != "L":
        ci += 2
    elif g(ci + 1) != "L":
        ci += 1
    ci += 1
    # left: marker 'R'
    if g(ci + 1) != "R" and g(ci + 2) != "R":
        ci += 2
    elif g(ci + 1) != "R":
        ci += 1
    ci += 1
    # right: trailing digits
    if g(ci + 1).isdigit() and g(ci + 2).isdigit():
        ci += 2
    elif g(ci + 1).isdigit():
        ci += 1
    ci += 1
    return ci


def _area_line_min_width(line: str, w: int) -> int:
    """Minimum width an area line needs so the C++ parser never reads past it.

    The parser starts at size = 3*w+1 and grows it by (cell_len - 2) per compass cell
    (and +1 per 3-char S cell).  A trimmed archive line is shorter than the final size,
    which makes read_puzzle() read past the string end (garbage, or a crash — e.g. the
    two 6-compass segfaults).  Node lines are fixed at 3*w+1; only area lines grow.
    """
    size = 3 * w + 1
    j = 0
    next_status = 0  # 0 = line char, 1 = area cell
    n = len(line)
    while j < size:
        if next_status == 0:
            j += 1
            next_status = 1
        else:
            if j < n and line[j] == "S" and j + 2 < n and line[j + 2].isdigit():
                j += 3
                size += 1
            elif j < n and line[j] == "U":
                end = _compass_end(j, line)
                size += end - j - 2
                j = end
            else:
                j += 2
            next_status = 0
    return size


def _pad_grid_lines(rows: list[str], w: int) -> list[str]:
    """Pad each grid line so the C++ parser stays within bounds.

    Node lines (even index) are read at fixed offsets 0,3,6,... and need 3*w+1 chars.
    Area lines (odd index) grow as the parser walks compass/S cells, so pad each to its
    own computed minimum width.  Longer lines (compass content) are never truncated.
    """
    min_node = 3 * w + 1
    result = []
    for i, ln in enumerate(rows):
        if i % 2 == 0:
            need = min_node
        else:
            need = _area_line_min_width(ln, w)
        result.append(ln if len(ln) >= need else ln.ljust(need))
    return result


def build_puz(p: dict) -> str:
    """Render one archive puzzle dict as a complete .puz file (text)."""
    w, h = int(p["width"]), int(p["height"])
    lines: list[str] = []

    # Optional headers, all skipped by the C++ parser.
    lines.append("VERSION 1")
    lines.append("PUZZLE_VERSION 2")
    lines.append("DIFFICULTY %s" % p["difficulty"])

    # Shape definitions: the C++ assigns indices 1..n by insertion order, and the
    # archive's S1/S2 cell markers reference those ids, so emit in id order.
    # The archive stores each shape grid with trailing spaces trimmed; the C++
    # SHAPE parser sizes the shape as max(last-row length, n_rows), so an
    # un-padded last row silently drops cells.  Pad every row to the max width.
    for shape in p.get("shapes", []):
        grid = shape["grid"]
        pad = max((len(r) for r in grid), default=0)
        lines.append("SHAPE %s %d" % (shape["id"], len(grid)))
        lines.extend(r.ljust(pad) for r in grid)
    if p.get("shape_bank"):
        # Enables 'predefined shapes only' in the solver.  The count must sit on the
        # same line (main() consumes the rest of it with getline).
        lines.append("SHAPE_BANK %d" % len(p["shape_bank"]))

    # Rule / area-bound directives.
    for key, directive in RULE_FLAGS:
        if p.get(key):
            lines.append(directive)
    for key, directive in AREA_BOUNDS:
        if p.get(key) is not None:
            lines.append("%s %s" % (directive, p[key]))

    lines.append("DIMENSIONS %d %d" % (w, h))

    # Grid sections: pad each line with trailing spaces (the archive trims them).
    # Node lines need 3*width+1 chars; area lines may need more because compass/S
    # cells widen them in the C++ parser.  Solutions are fixed-width boundary
    # renderings, so 3*width+1 is enough.  Longer lines are never truncated.
    grid_width = 3 * w + 1
    lines.append("PUZZLE")
    lines.extend(_pad_grid_lines(p["puzzle_grid"], w))
    lines.append("SOLUTION")
    for ln in p["solution"]:
        lines.append(ln if len(ln) >= grid_width else ln.ljust(grid_width))

    return "\n".join(lines) + "\n"


def load_archive() -> dict[str, dict]:
    """id -> archive puzzle dict."""
    return {str(p["id"]): p for p in json.load(open(ARCHIVE_JSON))}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out",
        default="aog_puzzles",
        help="output root (default: aog_puzzles)",
    )
    parser.add_argument("--ids", nargs="*", help="only convert these ids (debug)")
    args = parser.parse_args()

    archive = load_archive()
    if args.ids:
        items = [(i, archive[i]) for i in args.ids if i in archive]
    else:
        items = sorted(archive.items())

    converted = 0
    for pid, p in items:
        if not p["zone"] or not p["zone"].startswith("Zone"):
            continue  # skip the non-game 'Puzzles'/'Data' entries
        out_dir = os.path.join(args.out, p["zone"], p["type"])
        os.makedirs(out_dir, exist_ok=True)
        dest = os.path.join(out_dir, f"{pid}.puz")
        text = build_puz(p)
        if not p.get("solution"):
            # The archive has no official solution for this puzzle (0067, 1130);
            # keep a SOLUTION baked by fix_puz_solutions.py if one already exists,
            # otherwise the next batch_run would report it 'wrong'.
            existing_sol = None
            try:
                _head, _sep, existing_sol = open(dest, encoding="utf-8").read().partition("\nSOLUTION\n")
            except FileNotFoundError:
                pass
            if existing_sol and existing_sol.strip():
                text = text.split("\nSOLUTION\n", 1)[0] + "\nSOLUTION\n" + existing_sol
        with open(dest, "w", encoding="utf-8") as f:
            f.write(text)
        converted += 1

    print(f"转换 {converted} 个谜题 -> {os.path.abspath(args.out)}")
    if not args.ids:
        print("运行验证: cd third_party/AoG_Solver && ./batch_run.sh ../aog_puzzles/Zone1")


if __name__ == "__main__":
    main()
