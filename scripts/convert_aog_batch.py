"""Batch-convert aog puzzles to our JSON format using the aog Rust parser.

Usage: python scripts/convert_aog_batch.py
"""

import json
import os
import re
import subprocess
import sys

AOG_PARSER = os.path.join("third_party", "aog", "target", "debug", "aog_solver.exe")
AOG_SAMPLES = os.path.join("third_party", "aog", "samples")
OUT_DIR = os.path.join("puzzles", "reference")

# Known shape bank entries (from aog's standard shapes)
NAMED_SHAPES: dict[str, list[list[int]]] = {
    "O": [[0, 0], [0, 1], [1, 0], [1, 1]],
    "I": [[0, 0], [0, 1], [0, 2], [0, 3]],
    "T": [[0, 0], [0, 1], [0, 2], [1, 1]],
    "L": [[0, 0], [1, 0], [2, 0], [2, 1]],
    "S": [[0, 1], [0, 2], [1, 0], [1, 1]],
    "Z": [[0, 0], [0, 1], [1, 1], [1, 2]],
    "A": [[0, 0], [0, 1], [1, 1], [1, 2]],
    "P": [[0, 0], [0, 1], [1, 0], [1, 1], [2, 0]],
    "W": [[0, 0], [0, 1], [1, 1], [1, 2], [2, 2]],
    "F": [[0, 1], [1, 0], [1, 1], [1, 2], [2, 0]],
    "X": [[0, 1], [1, 0], [1, 1], [1, 2], [2, 1]],
}

# Archive-style rule string key -> aog rule type.
_AOG_RULE_MAP: dict[str, str] = {
    "solitude": "solitary",
    "boxy": "block",
    "non-boxy": "non_block",
    "bricky": "brick",
    "loopy": "ring",
    "mismatch": "different",
    "match": "same",
    "size separation": "differentiation",
}

# Watchtower marker char -> count.
_WT_MAP: dict[str, int] = {"!": 1, "@": 2, "#": 3, "$": 4}


def _aog_shape_bank_rule(rule_str: str) -> dict:
    names = rule_str[len("shape bank") :].strip().split()
    shapes = [
        NAMED_SHAPES.get(n, [list(reversed(divmod(idx, 2))) for idx in range(4)]) for n in names
    ]
    return {"type": "shape_pool", "params": {"shapes": [s for s in shapes if s]}}


def _aog_precision_rule(rule_str: str) -> dict:
    v = int("".join(c for c in rule_str if c.isdigit()))
    return {"type": "precise", "params": {"area": v}}


def _aog_minimum_rule(rule_str: str) -> dict:
    v = int("".join(c for c in rule_str if c.isdigit()))
    return {"type": "range", "params": {"min": v, "max": 999}}


def _aog_maximum_rule(rule_str: str) -> dict:
    v = int("".join(c for c in rule_str if c.isdigit()))
    return {"type": "range", "params": {"min": 1, "max": v}}


_AOG_PREFIX_HANDLERS: dict[str, callable] = {
    "shape bank": _aog_shape_bank_rule,
    "precision": _aog_precision_rule,
    "minimum": _aog_minimum_rule,
    "maximum": _aog_maximum_rule,
}


def _aog_rule_prefix(rule_str: str) -> str | None:
    for key in _AOG_PREFIX_HANDLERS:
        if rule_str.startswith(key):
            return key
    return None


def parse_aog_rule(rule_str: str) -> dict | None:
    """Convert aog rule string to our Rule dict."""
    r = rule_str.strip()
    prefix = _aog_rule_prefix(r)
    if prefix is not None:
        return _AOG_PREFIX_HANDLERS[prefix](r)
    for k, v in _AOG_RULE_MAP.items():
        if r.startswith(k):
            return {"type": v, "params": {}}
    return None


def cell_addr_to_rc(addr: str) -> tuple[int, int]:
    """a1 → (0,0), b3 → (1,2)"""
    row = ord(addr[0].lower()) - ord("a")
    col = int(addr[1:]) - 1
    return row, col


_LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"


def _is_blank(content: str) -> bool:
    return content == "_"


def _is_palisade(content: str) -> bool:
    return content.startswith("p")


def _is_bare_c(content: str) -> bool:
    return content.startswith("c") and len(content) == 1


def _is_number(content: str) -> bool:
    return content.isdigit()


def _is_letter(content: str) -> bool:
    return len(content) == 1 and content in _LETTERS


def _is_compass(content: str) -> bool:
    return any(d in content for d in "NEWS") and any(c.isdigit() for c in content)


def _no_clue(_content: str) -> dict:
    return {}


def _symbol_clue(content: str) -> dict:
    return {"symbol": content}


def _number_clue(content: str) -> dict:
    return {"number": int(content)}


def _compass_clue(content: str) -> dict:
    return {"compass": _parse_compass_clue(content)}


# Ordered (predicate, builder) table: first match wins, mirroring the original chain.
_CELL_CLUE_PARSERS: list[tuple] = [
    (_is_blank, _no_clue),
    (_is_palisade, _no_clue),  # Palisade — skip for now (not supported)
    (_is_bare_c, _symbol_clue),
    (_is_number, _number_clue),
    (_is_letter, _symbol_clue),
    (_is_compass, _compass_clue),  # Compass pattern: N1E2W3S4 etc.
]


def parse_cell_clue(content: str) -> dict:
    """Parse a cell clue from aog format to our format."""
    for matches, build in _CELL_CLUE_PARSERS:
        if matches(content):
            return build(content)
    return {}


def _parse_compass_clue(content: str) -> dict:
    compass = {"up": -1, "down": -1, "left": -1, "right": -1}
    for d, v in re.findall(r"([NEWS])(\d+)", content):
        n = int(v)
        if d == "N":
            compass["up"] = n
        elif d == "S":
            compass["down"] = n
        elif d == "W":
            compass["left"] = n
        elif d == "E":
            compass["right"] = n
    return compass


def edge_addr_to_key(addr: str) -> tuple[int, int, int, int]:
    """ha1 → (0,0,0,1) horizontal edge at row 0, cols 0,1.
    va1 → (0,0,1,0) vertical edge at rows 0,1, col 0."""
    row = ord(addr[1].lower()) - ord("a")
    col = int(addr[2:]) - 1
    if addr[0] == "h":
        return (row, col, row, col + 1)
    else:
        return (row, col, row + 1, col)


def parse_edge_clue(content: str) -> dict | None:
    """Parse edge clue. Returns the constraint dict, or None (no constraint / pre-cut only)."""
    ct = content.strip()
    if ct == "d":
        return {"type": "heterogeneous", "value": None}
    if ct == "g":
        return {"type": "homogeneous", "value": None}
    if ct in (">", "<", "^", "v"):
        return {"type": "inequality", "value": 0}
    return None


def _aog_grid_size(cells_raw: dict) -> tuple[int, int]:
    max_row = 0
    max_col = 0
    for addr in cells_raw:
        r, c = cell_addr_to_rc(addr)
        max_row = max(max_row, r)
        max_col = max(max_col, c)
    return max_row + 1, max_col + 1


def _aog_parse_rules(rules_raw: list[str]) -> tuple[list[dict], list]:
    rules: list[dict] = []
    shape_pool: list = []
    for r in rules_raw:
        parsed = parse_aog_rule(r)
        if parsed:
            if parsed["type"] == "shape_pool":
                shape_pool = parsed["params"].get("shapes", [])
            rules.append(parsed)
    return rules, shape_pool


def _aog_parse_cells(cells_raw: dict, height: int, width: int) -> list[dict]:
    cells: list[dict] = []
    for row in range(height):
        for col in range(width):
            addr = f"{chr(ord('a') + row)}{col + 1}"
            content = cells_raw.get(addr, "") if addr in cells_raw else ""
            cell = {"row": row, "col": col}
            # Check if this cell exists (empty string means blocked/hole)
            if content == "":
                cell["blocked"] = True
            else:
                cell.update(parse_cell_clue(content))
            cells.append(cell)
    return cells


def _aog_edge_coords(addr: str) -> tuple[int, int, int, int] | None:
    if "-" in addr:
        parts = addr.split("-")
    elif "|" in addr:
        parts = addr.split("|")
    else:
        return None
    r1, c1 = cell_addr_to_rc(parts[0])
    r2, c2 = cell_addr_to_rc(parts[1])
    return r1, c1, r2, c2


def _aog_parse_edges(edges_raw: dict) -> list[dict]:
    edges: list[dict] = []
    for addr, content in edges_raw.items():
        coords = _aog_edge_coords(addr)
        if coords is None:
            continue
        r1, c1, r2, c2 = coords
        edge = {"r1": r1, "c1": c1, "r2": r2, "c2": c2}
        # Content is either "-"/"|" (pre-cut only) or clue string
        edge["is_boundary"] = True  # aog edges with clues are always pre-cut
        if content not in ("-", "|"):
            constraint = parse_edge_clue(content)
            if constraint:
                edge["constraint"] = constraint
        edges.append(edge)
    return edges


def _aog_parse_vertices(vertices_raw: dict) -> list[dict]:
    vertices: list[dict] = []
    for addr, content in vertices_raw.items():
        # addr like "a1+b2"
        if "+" not in addr:
            continue
        parts = addr.split("+")
        r1, c1 = cell_addr_to_rc(parts[0])
        r2, c2 = cell_addr_to_rc(parts[1])
        # Vertex is at the corner: min(r1,r2) for row, corresponding col
        vr = min(r1, r2)
        vc = c1  # top-left corner of first cell
        wt = _WT_MAP.get(content.strip())
        if wt is None and content.strip().isdigit():
            wt = int(content.strip())
        if wt is not None:
            vertices.append({"row": vr, "col": vc, "watchtower": wt})
    return vertices


def _aog_outer_boundaries(height: int, width: int) -> list[dict]:
    outer_boundaries: list[dict] = []
    for r in range(height):
        outer_boundaries.append({"r1": r, "c1": 0, "r2": r + 1, "c2": 0})
        outer_boundaries.append({"r1": r, "c1": width, "r2": r + 1, "c2": width})
    for c in range(width):
        outer_boundaries.append({"r1": 0, "c1": c, "r2": 0, "c2": c + 1})
        outer_boundaries.append({"r1": height, "c1": c, "r2": height, "c2": c + 1})
    return outer_boundaries


def _aog_edge_constraint_types(edges: list[dict]) -> set[str]:
    types: set[str] = set()
    for e in edges:
        ct = e.get("constraint")
        if ct:
            types.add(ct.get("type"))
    return types


def _aog_auto_rules(
    cells: list[dict], edges: list[dict], vertices: list[dict], rules: list[dict]
) -> list[dict]:
    has_compass = any(c.get("compass") for c in cells)
    has_number = any(c.get("number") for c in cells)
    has_symbol = any(c.get("symbol") for c in cells)
    has_watchtower = any(v.get("watchtower") is not None for v in vertices)
    edge_types = _aog_edge_constraint_types(edges)

    existing_types = {r["type"] for r in rules}
    # A-E symbols in aog format can be either rose window symbols or
    # polyomino shape names; only add solitary if the aog rules requested it.
    auto = [
        (has_compass, "compass"),
        (has_number, "area"),
        (has_symbol, "solitary"),
        ("heterogeneous" in edge_types, "heterogeneous"),
        ("homogeneous" in edge_types, "homogeneous"),
        ("inequality" in edge_types, "inequality"),
        ("difference" in edge_types, "difference"),
        (has_watchtower, "watchtower"),
    ]
    for flag, rtype in auto:
        if flag and rtype not in existing_types:
            rules.append({"type": rtype, "params": {}})
    return rules


def convert_aog_json(aog_data: dict) -> dict | None:
    """Convert aog Rust parser JSON output to our format."""
    try:
        height, width = _aog_grid_size(aog_data.get("cells", {}))
    except Exception:
        return None

    rules, shape_pool = _aog_parse_rules(aog_data.get("rules", []))
    cells = _aog_parse_cells(aog_data.get("cells", {}), height, width)
    edges = _aog_parse_edges(aog_data.get("edges", {}))
    vertices = _aog_parse_vertices(aog_data.get("vertices", {}))
    outer_boundaries = _aog_outer_boundaries(height, width)

    rules = _aog_auto_rules(cells, edges, vertices, rules)

    return {
        "version": "1.0",
        "grid": {"height": height, "width": width},
        "cells": cells,
        "edges": edges,
        "vertices": vertices,
        "outer_boundaries": outer_boundaries,
        "rules": rules,
        "shape_pool": shape_pool,
    }


def _parse_sample(fname: str, path: str) -> tuple[dict | None, str | None]:
    """Run the aog Rust parser on one sample.

    Returns ``(aog_data, None)`` or ``(None, ready-to-print status line)``.
    """
    try:
        result = subprocess.run(
            [AOG_PARSER, "--parse", path],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode != 0:
            return None, f"FAIL {fname}: parser error\n    {result.stderr[:200]}"
        return json.loads(result.stdout), None
    except subprocess.TimeoutExpired:
        return None, f"SKIP {fname}: timeout"
    except json.JSONDecodeError as e:
        return None, f"FAIL {fname}: JSON error: {e}"
    except Exception as e:
        return None, f"FAIL {fname}: {e}"


def _convert_file(fname: str) -> bool:
    """Convert one sample file; returns True when a JSON puzzle was written."""
    path = os.path.join(AOG_SAMPLES, fname)
    name = os.path.splitext(fname)[0]

    aog_data, status = _parse_sample(fname, path)
    if status is not None:
        print(f"  {status}")
        return False

    our_data = convert_aog_json(aog_data)
    if our_data is None:
        print(f"  SKIP {fname}: conversion failed")
        return False

    h = our_data["grid"]["height"]
    w = our_data["grid"]["width"]
    if h < 2 or w < 2:
        print(f"  SKIP {fname}: {h}x{w} too small (min 2x2)")
        return False

    out_path = os.path.join(OUT_DIR, f"{name}.json")
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(our_data, f, ensure_ascii=False, indent=2)
    print(f"  OK  {fname}")
    return True


def batch_convert():
    if not os.path.exists(AOG_SAMPLES):
        print(f"Directory not found: {AOG_SAMPLES}")
        sys.exit(1)

    os.makedirs(OUT_DIR, exist_ok=True)

    txt_files = sorted(f for f in os.listdir(AOG_SAMPLES) if f.endswith(".txt"))

    ok = sum(1 for fname in txt_files if _convert_file(fname))

    print(f"\nConverted {ok}/{len(txt_files)} puzzles → {OUT_DIR}/")


if __name__ == "__main__":
    batch_convert()
