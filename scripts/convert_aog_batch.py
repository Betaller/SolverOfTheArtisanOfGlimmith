"""Batch-convert aog puzzles to our JSON format using the aog Rust parser.

Usage: python scripts/convert_aog_batch.py
"""

import json
import os
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


def parse_aog_rule(rule_str: str) -> dict | None:
    """Convert aog rule string to our Rule dict."""
    r = rule_str.strip()
    if r.startswith("shape bank"):
        names = r[len("shape bank"):].strip().split()
        shapes = [NAMED_SHAPES.get(n, [list(reversed(divmod(idx, 2))) for idx in range(4)]) for n in names]
        shapes = [s for s in shapes if s]
        return {"type": "shape_pool", "params": {"shapes": shapes}}
    if r.startswith("precision"):
        v = int(''.join(c for c in r if c.isdigit()))
        return {"type": "precise", "params": {"area": v}}
    if r.startswith("minimum"):
        v = int(''.join(c for c in r if c.isdigit()))
        return {"type": "range", "params": {"min": v, "max": 999}}
    if r.startswith("maximum"):
        v = int(''.join(c for c in r if c.isdigit()))
        return {"type": "range", "params": {"min": 1, "max": v}}
    rule_map = {
        "solitude": "solitary",
        "boxy": "block",
        "non-boxy": "non_block",
        "bricky": "brick",
        "loopy": "ring",
        "mismatch": "different",
        "match": "same",
        "size separation": "differentiation",
    }
    for k, v in rule_map.items():
        if r.startswith(k):
            return {"type": v, "params": {}}
    return None


def cell_addr_to_rc(addr: str) -> tuple[int, int]:
    """a1 → (0,0), b3 → (1,2)"""
    row = ord(addr[0].lower()) - ord('a')
    col = int(addr[1:]) - 1
    return row, col


def parse_cell_clue(content: str) -> dict:
    """Parse a cell clue from aog format to our format."""
    result: dict = {}
    if content == "_":
        return result
    if content.startswith("p"):
        # Palisade — skip for now (not supported)
        return result
    if content.startswith("c") and len(content) == 1:
        result["symbol"] = "c"
        return result
    if content.isdigit():
        result["number"] = int(content)
        return result
    if len(content) == 1 and content in "ABCDEFGHIJKLMNOPQRSTUVWXYZ":
        result["symbol"] = content
        return result
    # Compass pattern: N1E2W3S4 etc.
    if any(d in content for d in "NEWS") and any(c.isdigit() for c in content):
        compass = {"up": -1, "down": -1, "left": -1, "right": -1}
        m = re.findall(r"([NEWS])(\d+)", content)
        for d, v in m:
            n = int(v)
            if d == "N":
                compass["up"] = n
            elif d == "S":
                compass["down"] = n
            elif d == "W":
                compass["left"] = n
            elif d == "E":
                compass["right"] = n
        result["compass"] = compass
        return result
    return result


import re


def edge_addr_to_key(addr: str) -> tuple[int, int, int, int]:
    """ha1 → (0,0,0,1) horizontal edge at row 0, cols 0,1.
       va1 → (0,0,1,0) vertical edge at rows 0,1, col 0."""
    row = ord(addr[1].lower()) - ord('a')
    col = int(addr[2:]) - 1
    if addr[0] == 'h':
        return (row, col, row, col + 1)
    else:
        return (row, col, row + 1, col)


def parse_edge_clue(content: str, addr: str) -> dict | None:
    """Parse edge clue. addr is like 'ha1' or 'a1-a2'."""
    # Determine if horizontal or vertical
    if '-' in addr and '|' not in addr:
        # a1-a2 format → horizontal
        pass
    ct = content.strip()
    if ct == "d":
        return {"type": "heterogeneous", "value": None}
    if ct == "g":
        return {"type": "homogeneous", "value": None}
    if ct == ">" or ct == "<":
        return {"type": "inequality", "value": 0}
    if ct == "^" or ct == "v":
        return {"type": "inequality", "value": 0}
    if ct == "-" or ct == "|":
        return None  # pre-cut only
    return None


def convert_aog_json(aog_data: dict) -> dict | None:
    """Convert aog Rust parser JSON output to our format."""
    try:
        # Determine grid size from cell addresses
        cells_raw = aog_data.get("cells", {})
        max_row = 0
        max_col = 0
        for addr in cells_raw:
            r, c = cell_addr_to_rc(addr)
            max_row = max(max_row, r)
            max_col = max(max_col, c)
        height = max_row + 1
        width = max_col + 1
    except Exception:
        return None

    # Parse rules
    rules_raw = aog_data.get("rules", [])
    rules = []
    shape_pool = []
    for r in rules_raw:
        parsed = parse_aog_rule(r)
        if parsed:
            if parsed["type"] == "shape_pool":
                shape_pool = parsed["params"].get("shapes", [])
            rules.append(parsed)

    # Parse cells — ensure every grid position has an entry
    cells = []
    for row in range(height):
        for col in range(width):
            addr = f"{chr(ord('a')+row)}{col+1}"
            content = cells_raw.get(addr, "") if addr in cells_raw else ""
            cell = {"row": row, "col": col}
            # Check if this cell exists (empty string means blocked/hole)
            if content == "":
                cell["blocked"] = True
            else:
                cell.update(parse_cell_clue(content))
            cells.append(cell)

    # Parse edges
    edges_raw = aog_data.get("edges", {})
    edges = []
    for addr, content in edges_raw.items():
        if '-' in addr:
            parts = addr.split('-')
            r1, c1 = cell_addr_to_rc(parts[0])
            r2, c2 = cell_addr_to_rc(parts[1])
        elif '|' in addr:
            parts = addr.split('|')
            r1, c1 = cell_addr_to_rc(parts[0])
            r2, c2 = cell_addr_to_rc(parts[1])
        else:
            continue

        edge = {"r1": r1, "c1": c1, "r2": r2, "c2": c2}
        # Content is either "-"/"|" (pre-cut only) or clue string
        if content in ("-", "|"):
            edge["is_boundary"] = True
        else:
            edge["is_boundary"] = True  # aog edges with clues are always pre-cut
            constraint = parse_edge_clue(content, addr)
            if constraint:
                edge["constraint"] = constraint
        edges.append(edge)

    # Parse vertices
    vertices_raw = aog_data.get("vertices", {})
    vertices = []
    for addr, content in vertices_raw.items():
        # addr like "a1+b2"
        if '+' in addr:
            parts = addr.split('+')
            r1, c1 = cell_addr_to_rc(parts[0])
            r2, c2 = cell_addr_to_rc(parts[1])
            # Vertex is at the corner: min(r1,r2) for row, corresponding col
            vr = min(r1, r2)
            vc = c1  # top-left corner of first cell
            wt_map = {"!": 1, "@": 2, "#": 3, "$": 4}
            wt = wt_map.get(content.strip(), None)
            if wt is None and content.strip().isdigit():
                wt = int(content.strip())
            if wt is not None:
                vertices.append({"row": vr, "col": vc, "watchtower": wt})

    # Outer boundaries (all perimeter edges) — format as list of dicts
    outer_boundaries = []
    for r in range(height):
        outer_boundaries.append({"r1": r, "c1": 0, "r2": r + 1, "c2": 0})
        outer_boundaries.append({"r1": r, "c1": width, "r2": r + 1, "c2": width})
    for c in range(width):
        outer_boundaries.append({"r1": 0, "c1": c, "r2": 0, "c2": c + 1})
        outer_boundaries.append({"r1": height, "c1": c, "r2": height, "c2": c + 1})

    # Auto-detect rules from clues
    has_compass = any(c.get("compass") for c in cells)
    has_number = any(c.get("number") for c in cells)
    has_symbol = any(c.get("symbol") for c in cells)
    has_heterogeneous = any(e.get("constraint", {}).get("type") == "heterogeneous" for e in edges)
    has_homogeneous = any(e.get("constraint", {}).get("type") == "homogeneous" for e in edges)
    has_inequality = any(e.get("constraint", {}).get("type") == "inequality" for e in edges)
    has_difference = any(e.get("constraint", {}).get("type") == "difference" for e in edges)
    has_watchtower = any(v.get("watchtower") is not None for v in vertices)

    existing_types = {r["type"] for r in rules}
    if has_compass and "compass" not in existing_types:
        rules.append({"type": "compass", "params": {}})
    if has_number and "area" not in existing_types:
        rules.append({"type": "area", "params": {}})
    if (has_symbol or has_compass) and "solitary" not in existing_types:
        symbol_types = set()
        for c in cells:
            s = c.get("symbol")
            if s and len(s) == 1 and s in "ABCDE":
                symbol_types.add(s)
        if symbol_types and "rose_window" not in existing_types and "same" not in existing_types:
            rules.append({"type": "rose_window", "params": {"symbol_types": sorted(symbol_types)}})
    if has_heterogeneous and "heterogeneous" not in existing_types:
        rules.append({"type": "heterogeneous", "params": {}})
    if has_homogeneous and "homogeneous" not in existing_types:
        rules.append({"type": "homogeneous", "params": {}})
    if has_inequality and "inequality" not in existing_types:
        rules.append({"type": "inequality", "params": {}})
    if has_difference and "difference" not in existing_types:
        rules.append({"type": "difference", "params": {}})
    if has_watchtower and "watchtower" not in existing_types:
        rules.append({"type": "watchtower", "params": {}})

    result = {
        "version": "1.0",
        "grid": {"height": height, "width": width},
        "cells": cells,
        "edges": edges,
        "vertices": vertices,
        "outer_boundaries": outer_boundaries,
        "rules": rules,
        "shape_pool": shape_pool,
    }
    return result


def batch_convert():
    if not os.path.exists(AOG_SAMPLES):
        print(f"Directory not found: {AOG_SAMPLES}")
        sys.exit(1)

    os.makedirs(OUT_DIR, exist_ok=True)

    txt_files = sorted(f for f in os.listdir(AOG_SAMPLES) if f.endswith(".txt"))

    ok = 0
    for fname in txt_files:
        path = os.path.join(AOG_SAMPLES, fname)
        name = os.path.splitext(fname)[0]

        # Use aog Rust parser
        try:
            result = subprocess.run(
                [AOG_PARSER, "--parse", path],
                capture_output=True, text=True, timeout=10,
            )
            if result.returncode != 0:
                print(f"  FAIL {fname}: parser error\n    {result.stderr[:200]}")
                continue
            aog_data = json.loads(result.stdout)
        except subprocess.TimeoutExpired:
            print(f"  SKIP {fname}: timeout")
            continue
        except json.JSONDecodeError as e:
            print(f"  FAIL {fname}: JSON error: {e}")
            continue
        except Exception as e:
            print(f"  FAIL {fname}: {e}")
            continue

        our_data = convert_aog_json(aog_data)
        if our_data is None:
            print(f"  SKIP {fname}: conversion failed")
            continue

        h = our_data["grid"]["height"]
        w = our_data["grid"]["width"]
        if h < 2 or w < 2:
            print(f"  SKIP {fname}: {h}x{w} too small (min 2x2)")
            continue

        out_path = os.path.join(OUT_DIR, f"{name}.json")
        with open(out_path, "w", encoding="utf-8") as f:
            json.dump(our_data, f, ensure_ascii=False, indent=2)
        ok += 1
        print(f"  OK  {fname}")

    print(f"\nConverted {ok}/{len(txt_files)} puzzles → {OUT_DIR}/")


if __name__ == "__main__":
    batch_convert()
