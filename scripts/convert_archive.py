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

from src.models.board import CompassClue, Shape
from src.io.puzzle_codec import dict_to_puzzle

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


def parse_shapes(p: dict) -> dict[int, Shape]:
    """Map shape id -> normalized Shape from the archive 'shapes' list."""
    result: dict[int, Shape] = {}
    for shape in p.get("shapes", []):
        grid = shape.get("grid", [])
        cells = {
            (r, c)
            for r, row in enumerate(grid)
            for c, ch in enumerate(row)
            if ch == "#"
        }
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
    return CompassClue(
        up=values["U"], down=values["D"], left=values["L"], right=values["R"]
    )


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
            if first == "U":
                m = _COMPASS_RE.search(row[pos:])
                end = pos + m.end() if m else pos + 2
                cells.append(row[pos:end])
                pos = end
            elif first == "S":
                # greedy: read S + all consecutive digits, then back off so the
                # id exists in the shape table (handles multi-digit ids like S10)
                m = re.match(r"S(\d+)", row[pos:])
                if m:
                    digits = m.group(1)
                    n = len(digits)
                    while n > 1 and int(digits[:n]) not in shape_ids:
                        n -= 1
                    end = pos + 1 + n
                    cells.append(row[pos:end])
                    pos = end
                else:
                    cells.append(row[pos:pos + 2])
                    pos += 2
            else:
                cells.append(row[pos:pos + 2])
                pos += 2
    while len(cells) < width:
        cells.append("  ")
    while len(walls) < width + 1:
        walls.append(" ")
    return cells, walls


def _shape_list(shape: Shape) -> list[list[int]]:
    return sorted((r, c) for r, c in shape.cells)


def _parse_puzzle(p: dict) -> dict:
    width, height = int(p["width"]), int(p["height"])
    lines = [ln.ljust(3 * width + 1, " ") for ln in p.get("puzzle_grid", [])]
    if len(lines) < 2 * height + 1:
        lines.extend([" " * (3 * width + 1)] * (2 * height + 1 - len(lines)))

    shapes = parse_shapes(p)
    shape_ids = set(shapes)
    is_spr = bool(p.get("one_symbol_per_region"))
    has_shape_bank = bool(p.get("shape_bank"))

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

    has_compass = False
    has_fence = False
    has_numbers = False
    has_shape_pattern = False

    # --- cell rows (odd indices) ---
    for r in range(height):
        cells_row, walls = _parse_cell_row(lines[2 * r + 1], width, shape_ids)
        for c in range(width):
            content = cells_row[c]
            cell = cell_map[(r, c)]
            s = content.strip()
            if not s or content == "  ":
                cell["blocked"] = True
                fillable[r][c] = False
            elif content == "..":
                pass
            else:
                # clues keep their structured form; in 1-symbol-per-region
                # puzzles every clue also counts as a symbol marker
                if is_spr:
                    cell["symbol"] = content
                if re.fullmatch(r"\d\d", content):
                    cell["number"] = int(content)
                    has_numbers = True
                elif re.fullmatch(r"S\d+", content):
                    sid = int(content[1:])
                    if sid in shapes and not is_spr:
                        cell["shape_pattern"] = _shape_list(shapes[sid])
                        has_shape_pattern = True
                elif content in FENCE_PATTERNS:
                    cell["fence_pattern"] = _shape_list(
                        Shape(cells=FENCE_PATTERNS[content])
                    )
                    has_fence = True
                elif re.fullmatch(r"P[1-9]", content):
                    if not is_spr:
                        cell["symbol"] = content
                elif content[0] in "UDLR" and re.fullmatch(r"[UDLR\d]+", content):
                    comp = _compass_from_str(content)
                    cell["compass"] = {
                        "up": comp.up,
                        "down": comp.down,
                        "left": comp.left,
                        "right": comp.right,
                    }
                    has_compass = True
            # unknown content: keep cell fillable without clue

        for c in range(width + 1):
            ch = walls[c]
            if c == 0 or c == width:
                continue
            key = (r, c - 1, r, c)
            _apply_edge(edge_map[key], ch, cell_map, fillable, r, c - 1, r, c)

    # --- wall rows (even indices): horizontal edges + watchtower corners ---
    for r in range(height + 1):
        row = lines[2 * r]
        for c in range(width):
            seg = row[3 * c + 1:3 * c + 3]
            if 1 <= r <= height - 1:
                _apply_edge(edge_map[(r - 1, c, r, c)], seg, cell_map, fillable,
                            r - 1, c, r, c)
        if 1 <= r <= height - 1:
            for c in range(1, width):
                ch = row[3 * c] if 3 * c < len(row) else " "
                if ch.isdigit():
                    cell_map.setdefault("__vertices", {})
                    cell_map["__vertices"][(r - 1, c - 1)] = int(ch)

    # --- collect edges ---
    edges: list[dict] = []
    for e in edge_map.values():
        if e.get("is_boundary") or e.get("constraint") is not None:
            edges.append(e)
        else:
            r1, c1, r2, c2 = e["r1"], e["c1"], e["r2"], e["c2"]
            # gap (no glyph) between two fillable cells => forced boundary
            if (fillable[r1][c1] and fillable[r2][c2]
                    and not _has_glyph(e)):
                e["is_boundary"] = True
                edges.append(e)

    constraint_types = {
        e["constraint"]["type"] for e in edges if e.get("constraint")
    }

    vertices: list[dict] = []
    vt = cell_map.get("__vertices", {})
    for (vr, vc), n in sorted(vt.items()):
        vertices.append({"row": vr, "col": vc, "watchtower": n})

    outer_boundaries = []
    for c in range(width):
        outer_boundaries.append({"r1": 0, "c1": c, "r2": 0, "c2": c + 1})
        outer_boundaries.append({"r1": height, "c1": c, "r2": height, "c2": c + 1})
    for r in range(height):
        outer_boundaries.append({"r1": r, "c1": 0, "r2": r + 1, "c2": 0})
        outer_boundaries.append({"r1": r, "c1": width, "r2": r + 1, "c2": width})

    rules = build_rules(
        p, shapes, has_compass, has_fence, has_numbers, constraint_types,
        bool(vt), is_spr, has_shape_bank, has_shape_pattern,
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


def _apply_edge(entry: dict, glyph: str, cell_map: dict, fillable: list,
                r1: int, c1: int, r2: int, c2: int) -> None:
    entry["_glyph"] = glyph != "  " and glyph != " "
    if glyph in ("##", "#"):
        entry["is_boundary"] = True
        entry["_glyph"] = True
    elif glyph in ("==", "="):
        entry["constraint"] = {"type": "homogeneous"}
    elif glyph in ("!!", "!"):
        entry["constraint"] = {"type": "heterogeneous"}
    elif glyph in ("^^", "<", "^"):
        entry["constraint"] = {"type": "inequality"}
    elif glyph in ("vv", ">", "v"):
        entry["constraint"] = {"type": "inequality", "value": 1}
    elif re.fullmatch(r"-\d", glyph):
        entry["constraint"] = {"type": "difference", "value": int(glyph[1])}
    elif glyph.isdigit():
        entry["constraint"] = {"type": "difference", "value": int(glyph)}
    elif glyph in ("--", "|"):
        pass
    elif glyph == "  " or glyph == " ":
        pass
    else:
        # e.g. a shifted cell char; ignore unknown wall glyph
        pass


def build_rules(p: dict, shapes: dict[int, Shape], has_compass: bool,
                has_fence: bool, has_numbers: bool,
                constraint_types: set[str], has_watchtower: bool,
                is_spr: bool, has_shape_bank: bool,
                has_shape_pattern: bool) -> list[dict]:
    rules: list[dict] = []

    if has_shape_bank and shapes:
        bank = p["shape_bank"]
        pool = [shapes[i] for i in bank if i in shapes] or list(shapes.values())
        rules.append({
            "type": "shape_pool",
            "params": {"shapes": [_shape_list(s) for s in pool]},
        })

    if has_shape_pattern:
        rules.append({"type": "puzzle_piece"})

    for key, rule_type in (
        ("all_shapes_same", "same"),
        ("all_shapes_different", "different"),
        ("adjacent_shapes_different", "mixed"),
        ("adjacent_sizes_different", "differentiation"),
        ("only_rectangles", "block"),
        ("no_rectangles", "non_block"),
        ("no_4_way_intersections", "brick"),
        ("no_3_way_intersections", "ring"),
    ):
        if p.get(key):
            rules.append({"type": rule_type})

    if p.get("area_equals") is not None:
        rules.append({"type": "precise", "params": {"area": int(p["area_equals"])}})
    else:
        params: dict = {}
        if p.get("area_at_least") is not None:
            params["min"] = int(p["area_at_least"])
        if p.get("area_at_most") is not None:
            params["max"] = int(p["area_at_most"])
        if params:
            rules.append({"type": "range", "params": params})

    if has_numbers:
        rules.append({"type": "area"})
    if has_fence:
        rules.append({"type": "fence"})
    if has_compass:
        rules.append({"type": "compass"})
    if p.get("one_symbol_per_region"):
        rules.append({"type": "solitary"})
    if "inequality" in constraint_types:
        rules.append({"type": "inequality"})
    if "difference" in constraint_types:
        rules.append({"type": "difference"})
    if has_watchtower:
        rules.append({"type": "watchtower"})

    return rules


def convert_all(dry_run: bool = False) -> tuple[int, list[str]]:
    with open(ARCHIVE_PATH, encoding="utf-8") as f:
        data = json.load(f)

    if not dry_run:
        for zone in ("Zone1", "Zone2", "Zone3"):
            zone_dir = os.path.join(OUT_ROOT, zone)
            if os.path.isdir(zone_dir):
                for name in os.listdir(zone_dir):
                    path = os.path.join(zone_dir, name)
                    if os.path.isfile(path):
                        os.remove(path)
                    elif os.path.isdir(path):
                        for fname in os.listdir(path):
                            os.remove(os.path.join(path, fname))
                        os.rmdir(path)

    written = 0
    errors: list[str] = []
    per_zone: dict[str, dict[str, int]] = {}
    for p in data:
        zone = p.get("zone")
        if zone not in ("Zone1", "Zone2", "Zone3"):
            continue
        ptype = p.get("type", "misc")
        pid = p.get("id", "unknown")
        try:
            out = _parse_puzzle(p)
            dict_to_puzzle(out)  # round-trip validation
        except Exception as e:  # noqa: BLE001
            errors.append(f"{zone}/{ptype}/{pid}: {e}")
            continue
        if dry_run:
            written += 1
            continue
        out_dir = os.path.join(OUT_ROOT, zone, ptype)
        os.makedirs(out_dir, exist_ok=True)
        with open(os.path.join(out_dir, f"{pid}.json"), "w", encoding="utf-8") as f:
            json.dump(out, f, ensure_ascii=False, indent=2)
        written += 1
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
