"""Convert official puzzles from the archive (puzzles.json) to C++ AoG_Solver ansi input.

The archive (third_party/archiveofglimmith.github.io/puzzles.json) holds the raw
official grid text (puzzle_grid) plus the official solution for every puzzle, but
not the rule/shape-pool metadata.  That lives in `puzzles/official/**/*.json`.
This script merges the two:

  * puzzle_grid        -> C++ PUZZLE grid (already in AoG ansi layout, 22 chars/row)
  * puzzle rules       -> C++ header directives (SHAPE_BANK / ALL_SHAPES_SAME / ...)
  * puzzle shape_pool  -> C++ SHAPE definitions

Output: `third_party/AoG_Solver/puzzles_ansi/{zone}/{id}.ansi`
Usage:
    python scripts/convert_puzzles_json_to_aog.py [--out third_party/AoG_Solver/puzzles_ansi]
"""
from __future__ import annotations

import argparse
import glob
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import scripts.convert_archive as ca  # noqa: E402

ARCHIVE_JSON = "third_party/archiveofglimmith.github.io/puzzles.json"


def shape_to_shape_lines(cells: list[list[int]]) -> list[str]:
    """Shape (relative cells) -> C++ `SHAPE <name> <nrow>` + nrow lines of # / space."""
    cells = sorted(tuple(c) for c in cells)
    min_r = min(r for r, _ in cells)
    min_c = min(c for _, c in cells)
    norm = sorted((r - min_r, c - min_c) for r, c in cells)
    max_r = max(r for r, _ in norm)
    max_c = max(c for _, c in norm)
    size = max(max_r, max_c) + 1
    grid = [[" "] * size for _ in range(size)]
    for r, c in norm:
        grid[r][c] = "#"
    lines = [f"SHAPE s{size} {size}"]
    lines.extend("".join(row) for row in grid)
    return lines


def rules_to_headers(rules: list[dict], shape_pool: list[list[list[int]]]) -> list[str]:
    headers: list[str] = []
    for r in rules:
        t = r.get("type")
        if t == "shape_pool":
            headers.append("SHAPE_BANK")
            # The C++ main reads SHAPE_BANK then consumes the *next* line via
            # getline; without a blank line here the following SHAPE directive
            # would be swallowed and the shape catalog would stay empty.
            headers.append("")
            for shape in shape_pool:
                headers.extend(shape_to_shape_lines(shape))
        elif t == "precise":
            area = r.get("params", {}).get("area")
            if area is not None:
                headers.append(f"AREA_EQUALS {area}")
        elif t == "range":
            p = r.get("params", {})
            if "min" in p:
                headers.append(f"AREA_AT_LEAST {p['min']}")
            if "max" in p:
                headers.append(f"AREA_AT_MOST {p['max']}")
        elif t == "block":
            headers.append("ONLY_RECTANGLES")
        elif t == "non_block":
            headers.append("NO_RECTANGLES")
        elif t == "same":
            headers.append("ALL_SHAPES_SAME")
        elif t == "different":
            headers.append("ALL_SHAPES_DIFFERENT")
        elif t == "differentiation":
            headers.append("ADJACENT_SIZES_DIFFERENT")
        elif t == "mixed":
            headers.append("ADJACENT_SHAPES_DIFFERENT")
        elif t == "solitary":
            headers.append("ONE_SYMBOL_PER_REGION")
        elif t == "brick":
            headers.append("NO_4_WAY_INTERSECTIONS")
        elif t == "ring":
            headers.append("NO_3_WAY_INTERSECTIONS")
        # fence / compass / watchtower / area / inequality / difference /
        # rose_window / puzzle_piece are encoded inside the grid, not as headers.
    return headers


def build_index() -> dict[str, dict]:
    """id -> official puzzle dict (puzzle_grid, width, height, solution, zone, type)."""
    idx: dict[str, dict] = {}
    for p in json.load(open(ARCHIVE_JSON)):
        idx[str(p["id"])] = p
    return idx


def find_project_json(pid: str) -> dict | None:
    for f in glob.glob(f"puzzles/official/**/{pid}.json", recursive=True):
        return json.load(open(f))
    return None


def build_shapes(proj: dict) -> list[dict]:
    """shape_pool -> list of {'id','grid'} for the C++ SHAPE directives."""
    result: list[dict] = []
    for i, s in enumerate(proj.get("shape_pool", [])):
        cells = {(r, c) for r, c in s}
        if not cells:
            continue
        min_r = min(r for r, _ in cells)
        min_c = min(c for _, c in cells)
        norm = sorted((r - min_r, c - min_c) for r, c in cells)
        size = max(max(r for r, _ in norm), max(c for _, c in norm)) + 1
        grid = [[" "] * size for _ in range(size)]
        for r, c in norm:
            grid[r][c] = "#"
        result.append({"id": i + 1, "grid": ["".join(row) for row in grid]})
    return result


def fence_type(fp: list[list[int]]) -> int:
    """3x3 fence pattern -> C++ F marker (0..4, 7)."""
    fp = set(tuple(x) for x in fp)
    n = sum(1 for rc in ((0, 1), (2, 1), (1, 0), (1, 2)) if rc in fp)
    opp = ((0, 1) in fp and (2, 1) in fp) or ((1, 0) in fp and (1, 2) in fp)
    if n == 0:
        return 0
    if n == 1:
        return 1
    if n == 2 and opp:
        return 2
    if n == 2:
        return 7
    if n == 3:
        return 3
    return 4


def compass_str(cp: dict) -> str:
    """Compass clue -> C++ U string (up/down/left/right with D/L/R markers)."""
    parts = ["U"]
    for attr, marker in (("up", "D"), ("down", "L"), ("left", "R"), ("right", None)):
        v = cp.get(attr)
        if v is None or v < 0:
            parts.append(marker if marker else "")
        else:
            parts.append(str(v))
    return "".join(parts)


def json_to_ansi_lines(j: dict, sid_map: dict) -> list[str]:
    """Rebuild the C++ ansi grid from the parsed puzzle JSON.

    node line (even):  vertex '+' at every 3rd char, vertical edges between
    area line (odd):   alternating horizontal edge + cell content (2-3+ chars)
    """
    h = j["grid"]["height"]
    w = j["grid"]["width"]
    h_edge: dict = {}
    v_edge: dict = {}
    for e in j["edges"]:
        r1, c1, r2, c2 = e["r1"], e["c1"], e["r2"], e["c2"]
        if r1 == r2:
            h_edge[(r1, min(c1, c2))] = e
        else:
            v_edge[(min(r1, r2), c1)] = e
    cells = {(c["row"], c["col"]): c for c in j["cells"]}

    def edge_char(e: dict | None) -> str:
        if e is None:
            return "-"
        if e.get("constraint"):
            ct = e["constraint"]["type"]
            val = e["constraint"].get("value")
            if ct == "difference":
                return str(val or 0)
            if ct == "heterogeneous":
                return "!"
            if ct == "homogeneous":
                return "="
            if ct == "inequality":
                # value==1 -> first endpoint larger -> '>' (left larger in C++)
                return ">" if val == 1 else "<"
        return "#" if e.get("is_boundary") else "-"

    def cell_content(c: dict) -> str:
        if c.get("blocked"):
            return "  "
        if c.get("number") is not None:
            return str(c["number"]).zfill(2)
        if c.get("shape_pattern"):  # puzzle_piece -> Sxx (C++ shape index)
            key = tuple(sorted(tuple(x) for x in c["shape_pattern"]))
            sid = sid_map.get(key)
            if sid is not None:
                return "S%d" % sid
        if c.get("fence_pattern"):  # fence -> Fx
            return "F%d" % fence_type(c["fence_pattern"])
        if c.get("compass"):  # compass -> U...
            return compass_str(c["compass"])
        if c.get("symbol"):  # rose / one-symbol -> Px
            return "P0"  # single-symbol puzzles use P0
        return ".."

    lines: list[str] = []
    for i in range(2 * h + 1):
        if i % 2 == 0:
            # node line: fixed positions, every 3rd char is a vertex
            row = ["-"] * (3 * w + 1)
            for k in range(w + 1):
                row[k * 3] = "+"
            r = i // 2 - 1
            for c in range(w):
                if 0 <= r < h - 1:
                    row[c * 3 + 2] = edge_char(v_edge.get((r, c)))
            lines.append("".join(row))
        else:
            # area line: variable-length cell content (C++ while-loop parser)
            r = i // 2
            s = []
            for c in range(w + 1):
                if c == 0 or c == w:
                    s.append("#")
                else:
                    s.append(edge_char(h_edge.get((r, c - 1))))
                if c < w:
                    s.append(cell_content(cells.get((r, c), {})))
            lines.append("".join(s))
    return lines


def convert(pid: str, archive: dict) -> str:
    proj = find_project_json(pid)
    if proj is None:
        raise FileNotFoundError(f"no puzzles/official JSON for {pid}")
    shapes_list = build_shapes(proj)
    ap = {
        "width": archive["width"],
        "height": archive["height"],
        "puzzle_grid": archive["puzzle_grid"],
        "shapes": shapes_list,
        "shape_bank": [s["id"] for s in shapes_list] if shapes_list else None,
        "one_symbol_per_region": any(
            r.get("type") == "solitary" for r in proj.get("rules", [])
        ),
    }
    j = ca._parse_puzzle(ap)

    # puzzle_piece (Sxx) shapes: each distinct shape_pattern becomes a SHAPE
    # definition; the cell marker references it by id.
    sid_map: dict = {}
    piece_shapes: list = []
    for c in j["cells"]:
        sp = c.get("shape_pattern")
        if sp:
            key = tuple(sorted(tuple(x) for x in sp))
            if key not in sid_map:
                sid_map[key] = len(shapes_list) + len(piece_shapes) + 1
                piece_shapes.append(sp)

    grid_lines = json_to_ansi_lines(j, sid_map)

    lines = [f"DIMENSIONS {archive['width']} {archive['height']}"]
    lines.extend(rules_to_headers(proj.get("rules", []), proj.get("shape_pool", [])))
    # puzzle_piece shape definitions (after the shape_pool SHAPE_BANK block).
    if piece_shapes:
        if not shapes_list:
            lines.append("SHAPE_BANK")
            lines.append("")
        for shape in piece_shapes:
            lines.extend(shape_to_shape_lines(shape))
    lines.append("PUZZLE")
    lines.extend(grid_lines)
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default="puzzles_ansi")
    parser.add_argument("--ids", nargs="*", help="only convert these ids (debug)")
    args = parser.parse_args()

    archive = build_index()
    if args.ids:
        items = [(i, archive[i]) for i in args.ids if i in archive]
    else:
        items = sorted(archive.items())

    converted = 0
    skipped: list[str] = []
    for pid, p in items:
        try:
            ansi = convert(pid, p)
        except FileNotFoundError:
            skipped.append(pid)
            continue
        zone = p["zone"]
        out_dir = os.path.join(args.out, zone)
        os.makedirs(out_dir, exist_ok=True)
        with open(os.path.join(out_dir, f"{pid}.ansi"), "w", encoding="utf-8") as f:
            f.write(ansi + "\n")
        converted += 1

    print(f"转换 {converted} 个谜题, 跳过 {len(skipped)} (无 puzzles/official JSON)")
    if skipped:
        print("跳过示例:", skipped[:10])


if __name__ == "__main__":
    main()
