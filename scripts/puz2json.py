"""
.puz to JSON converter for "The Artisan of Glimmith" puzzles.

Based on reverse-engineered .puz format from AoG_Solver by Neptune17.
Converts to SolverOfTheArtisanOfGlimmith JSON format.

Usage: python puz2json.py <input.puz> [output.json]
       python puz2json.py --batch <puzzles_dir> <output_dir>
"""

import re, json, sys, os, glob

# ─── Parser ────────────────────────────────────────────────

def parse_puz(text):
    """Parse .puz text content into structured data."""
    lines = text.split("\n")
    
    config = {
        "version": 1,
        "puzzle_version": 2,
        "difficulty": 1,
        "shapes": [],        # list of 2D arrays (1=filled)
        "shape_bank": False,
        "adjacent_shapes_different": False,
        "adjacent_sizes_different": False,
        "all_shapes_different": False,
        "only_rectangles": False,
        "no_rectangles": False,
        "one_symbol_per_region": False,
        "all_shapes_same": False,
        "no_4_way_intersections": False,
        "no_3_way_intersections": False,
        "area_equals": None,
        "area_at_least": None,
        "area_at_most": None,
        "dimensions": None,  # (width, height)
    }
    
    i = 0
    while i < len(lines):
        line = lines[i].strip()
        
        if line.startswith("VERSION"):
            config["version"] = int(line.split()[1])
        elif line.startswith("PUZZLE_VERSION"):
            config["puzzle_version"] = int(line.split()[1])
        elif line.startswith("DIFFICULTY"):
            config["difficulty"] = int(line.split()[1])
        elif line.startswith("SHAPE_BANK"):
            config["shape_bank"] = True
        elif line.startswith("ADJACENT_SHAPES_DIFFERENT"):
            config["adjacent_shapes_different"] = True
        elif line.startswith("ADJACENT_SIZES_DIFFERENT"):
            config["adjacent_sizes_different"] = True
        elif line.startswith("ALL_SHAPES_DIFFERENT"):
            config["all_shapes_different"] = True
        elif line.startswith("ONLY_RECTANGLES"):
            config["only_rectangles"] = True
        elif line.startswith("NO_RECTANGLES"):
            config["no_rectangles"] = True
        elif line.startswith("ONE_SYMBOL_PER_REGION"):
            config["one_symbol_per_region"] = True
        elif line.startswith("ALL_SHAPES_SAME"):
            config["all_shapes_same"] = True
        elif line.startswith("NO_4_WAY_INTERSECTIONS"):
            config["no_4_way_intersections"] = True
        elif line.startswith("NO_3_WAY_INTERSECTIONS"):
            config["no_3_way_intersections"] = True
        elif line.startswith("AREA_EQUALS"):
            v = int(line.split()[1])
            config["area_equals"] = v
            config["area_at_least"] = v
            config["area_at_most"] = v
        elif line.startswith("AREA_AT_LEAST"):
            config["area_at_least"] = int(line.split()[1])
        elif line.startswith("AREA_AT_MOST"):
            config["area_at_most"] = int(line.split()[1])
        elif line.startswith("COMMENT"):
            pass  # skip
        elif line.startswith("DIMENSIONS"):
            parts = line.split()
            w = int(parts[1])
            h = int(parts[2])
            config["dimensions"] = (w, h)
        elif line.startswith("SHAPE"):
            # Parse shape definition
            parts = line.split()
            shape_h = int(parts[2])  # SHAPE <index> <height>
            shape_rows = []
            i += 1
            while i < len(lines) and len(shape_rows) < shape_h:
                row_line = lines[i]
                # Pad to max width
                shape_rows.append(list(row_line))
                i += 1
            i -= 1  # will be incremented at end of loop
            
            # Convert to binary matrix
            max_w = max(len(r) for r in shape_rows) if shape_rows else 0
            matrix = []
            for r in shape_rows:
                row = [1 if c == "#" else 0 for c in r]
                row.extend([0] * (max_w - len(row)))
                matrix.append(row)
            config["shapes"].append(matrix)
            
        elif line.startswith("PUZZLE"):
            # Find the puzzle grid lines
            i += 1
            puzzle_start = i
            puzzle_lines = []
            while i < len(lines) and not lines[i].strip().startswith("SOLUTION"):
                puzzle_lines.append(lines[i])
                i += 1
            
            # Also read SOLUTION if present
            solution_lines = []
            if i < len(lines) and lines[i].strip().startswith("SOLUTION"):
                i += 1
                while i < len(lines):
                    solution_lines.append(lines[i])
                    i += 1
            
            return config, puzzle_lines, solution_lines
        
        i += 1
    
    return config, [], []


def parse_puzzle_grid(config, puzzle_lines):
    """Parse the ASCII puzzle grid into cells, edges, vertices."""
    W, H = config["dimensions"]
    
    cells = []
    edges = []
    vertices = []
    
    # Process area lines (cell data) and node lines (vertex/edge data)
    # The grid has 2*H+1 rows alternating node/area
    for r in range(H):
        area_line_idx = r * 2 + 1  # area line index (0-based in puzzle_lines)
        node_line_idx = r * 2      # node line above this area row
        
        if area_line_idx >= len(puzzle_lines):
            break
        
        area_line = puzzle_lines[area_line_idx]
        
        # Process cell content on area line
        # Format: col pattern is: [v_edge] [cell_2chars] [v_edge] [cell_2chars] ...
        # Col 0: vertical edge
        # Col 1-2: cell content (area clue)
        # Col 3: vertical edge
        # Col 4-5: cell content
        # etc.
        
        for c in range(W):
            # Cell content starts at column c*3 + 1 in the area line
            cell_pos = c * 3 + 1
            if cell_pos + 1 >= len(area_line):
                break
            
            c1 = area_line[cell_pos] if cell_pos < len(area_line) else " "
            c2 = area_line[cell_pos + 1] if cell_pos + 1 < len(area_line) else " "
            
            cell = {"row": r, "col": c}
            
            # Parse cell area clue
            area_code = parse_area_clue(c1, c2, area_line, cell_pos)
            if area_code:
                cell.update(area_code)
            
            cells.append(cell)
    
    # Parse edges from the grid
    # Horizontal edges: on area lines, the vertical edge chars
    # Vertical edges: on node lines, the horizontal edge chars
    
    # Horizontal edges: (r,c)-(r,c+1)
    for r in range(H):
        area_line_idx = r * 2 + 1
        if area_line_idx >= len(puzzle_lines):
            break
        area_line = puzzle_lines[area_line_idx]
        for c in range(W - 1):
            # Horizontal edge is between cells at same row
            # On area line, after cell content at c*3+1, edge at c*3+0 (next col's v_edge)
            # Actually, looking at the parser code:
            # On area lines, j=0,3,6,... are vertical edges (parse_line)
            # j=0 is leftmost v_edge, j=3 is between col 0 and col 1, etc.
            edge_pos = (c + 1) * 3  # position of vertical edge char between cells
            if edge_pos < len(area_line):
                ch = area_line[edge_pos]
                edge_data = {"r1": r, "c1": c, "r2": r, "c2": c + 1}
                edge_clue = parse_line_clue(ch)
                if edge_clue:
                    edge_data.update(edge_clue)
                edges.append(edge_data)
    
    # Vertical edges: (r,c)-(r+1,c)
    for r in range(H - 1):
        node_line_idx = r * 2 + 2  # node line between area lines
        if node_line_idx >= len(puzzle_lines):
            break
        node_line = puzzle_lines[node_line_idx]
        for c in range(W):
            # Vertical edge on node line: at position c*3+1, char between vertices
            edge_pos = c * 3 + 1
            if edge_pos < len(node_line):
                ch = node_line[edge_pos]
                edge_data = {"r1": r, "c1": c, "r2": r + 1, "c2": c}
                edge_clue = parse_line_clue(ch)
                if edge_clue:
                    edge_data.update(edge_clue)
                edges.append(edge_data)
    
    # Parse vertices
    for r in range(H - 1):
        node_line_idx = r * 2 + 2
        if node_line_idx >= len(puzzle_lines):
            break
        node_line = puzzle_lines[node_line_idx]
        for c in range(W - 1):
            vertex_pos = c * 3
            if vertex_pos < len(node_line):
                ch = node_line[vertex_pos]
                v = {"row": r, "col": c}
                if ch in "1234":
                    v["watchtower"] = int(ch)
                vertices.append(v)
    
    return cells, edges, vertices


def parse_area_clue(c1, c2, area_line, pos):
    """Parse area clue from 2 characters + possible compass extension."""
    result = {}
    
    if c1 == " " and c2 == " ":
        result["blocked"] = True
        return result
    
    if c1 == "." and c2 == ".":
        return result  # normal cell, no clues
    
    # Area number: digits
    if c1.isdigit() and c2.isdigit():
        val = int(c1) * 10 + int(c2)
        result["number"] = val
        return result
    
    # Shape index: S + digit(s)
    if c1 == "S":
        # SX = single digit, SnX = two digits
        if pos + 2 < len(area_line) and area_line[pos + 2] != "X":
            # Two digit: S + digit1 + digit2
            if c2.isdigit() and area_line[pos + 2].isdigit():
                val = int(c2) * 10 + int(area_line[pos + 2])
                result["shape_index"] = val
        elif c2.isdigit():
            result["shape_index"] = int(c2)
        return result
    
    # Slash pack: P + digit
    if c1 == "P" and c2.isdigit():
        result["slash_index"] = int(c2)
        return result
    
    # Palisade/fence: F + digit
    if c1 == "F":
        fence_map = {"0": 1, "1": 2, "2": 3, "3": 4, "7": 5, "4": 6}
        if c2 in fence_map:
            result["fence_index"] = fence_map[c2]
        return result
    
    # Compass: U + compass string
    if c1 == "U":
        result["compass_enabled"] = True
        # Parse compass values from the rest of the line
        compass = {"up": -1, "down": -1, "left": -1, "right": -1}
        idx = pos + 1  # start after 'U'
        # Parse up
        idx, compass["up"] = parse_compass_value(area_line, idx, "D")
        # Parse down
        idx, compass["down"] = parse_compass_value(area_line, idx, "L")
        # Parse left
        idx, compass["left"] = parse_compass_value(area_line, idx, "R")
        # Parse right
        _, compass["right"] = parse_compass_value(area_line, idx, None)
        result["compass"] = compass
        return result
    
    return result


def parse_compass_value(line, idx, next_dir):
    """Parse a compass direction value. Returns (new_idx, value)."""
    if idx >= len(line):
        return idx, -1
    
    # Check if current position has the direction marker
    if line[idx] == next_dir if next_dir else False:
        idx += 1
    
    # Parse number (1-2 digits)
    if idx < len(line) and line[idx].isdigit():
        if idx + 1 < len(line) and line[idx + 1].isdigit():
            val = int(line[idx]) * 10 + int(line[idx + 1])
            idx += 2
        else:
            val = int(line[idx])
            idx += 1
        # Skip direction marker
        if idx < len(line) and line[idx] == next_dir:
            idx += 1
        return idx, val
    
    return idx, -1


def parse_line_clue(ch):
    """Parse edge clue character."""
    if ch == " ":
        return None  # no edge (out of bounds or no constraint)
    if ch == "#":
        return None  # blocked
    if ch == "|" or ch == "-":
        return None  # normal edge, no constraint
    if ch == "=":
        return {"constraint": {"type": "homogeneous"}}
    if ch == "!":
        return {"constraint": {"type": "heterogeneous"}}
    if ch in "<^":
        return {"constraint": {"type": "inequality"}}
    if ch in ">v":
        return {"constraint": {"type": "inequality", "value": 1}}  # reversed
    if ch.isdigit():
        val = int(ch) + 1  # difference value = digit + 1
        return {"constraint": {"type": "difference", "value": val}}
    return None


# ─── JSON Builder ──────────────────────────────────────────

def build_json(config, cells, edges, vertices):
    """Build Solver project JSON from parsed data."""
    W, H = config["dimensions"]
    
    # Build shape pool
    shape_pool = []
    for shape_matrix in config["shapes"]:
        shape_cells = []
        for r_idx, row in enumerate(shape_matrix):
            for c_idx, val in enumerate(row):
                if val == 1:
                    shape_cells.append([r_idx, c_idx])
        if shape_cells:
            # Normalize to origin
            min_r = min(c[0] for c in shape_cells)
            min_c = min(c[1] for c in shape_cells)
            normalized = [[r - min_r, c - min_c] for r, c in shape_cells]
            shape_pool.append(sorted(normalized))
    
    # Build rules
    rules = []
    
    if config["shape_bank"] and shape_pool:
        rules.append({"type": "shape_pool", "params": {"shapes": shape_pool}})
    
    if config["only_rectangles"]:
        rules.append({"type": "block"})
    if config["no_rectangles"]:
        rules.append({"type": "non_block"})
    if config["all_shapes_different"]:
        rules.append({"type": "different"})
    if config["all_shapes_same"]:
        rules.append({"type": "same"})
    if config["adjacent_shapes_different"]:
        rules.append({"type": "mixed"})
    if config["adjacent_sizes_different"]:
        rules.append({"type": "differentiation"})
    if config["one_symbol_per_region"]:
        rules.append({"type": "solitary"})
    if config["no_4_way_intersections"]:
        rules.append({"type": "brick"})
    if config["no_3_way_intersections"]:
        rules.append({"type": "ring"})
    if config["area_equals"] is not None:
        rules.append({"type": "precise", "params": {"area": config["area_equals"]}})
    elif config["area_at_least"] is not None or config["area_at_most"] is not None:
        params = {}
        if config["area_at_least"] is not None:
            params["min"] = config["area_at_least"]
        if config["area_at_most"] is not None:
            params["max"] = config["area_at_most"]
        if params:
            rules.append({"type": "range", "params": params})
    
    # Check if any cell has compass → add compass rule
    has_compass = any(c.get("compass_enabled") for c in cells)
    if has_compass:
        rules.append({"type": "compass"})
    
    # Process cells for JSON format
    # Rearrange to proper order (row-major)
    cell_dict = {}
    for c in cells:
        key = (c["row"], c["col"])
        cell_dict[key] = c
    
    json_cells = []
    for r in range(H):
        for c in range(W):
            key = (r, c)
            original = cell_dict.get(key, {})
            cell = {"row": r, "col": c}
            
            if original.get("blocked"):
                cell["blocked"] = True
            
            if original.get("number"):
                cell["number"] = original["number"]
            
            if original.get("compass"):
                comp = original["compass"]
                cell["compass"] = {
                    "up": comp["up"],
                    "down": comp["down"],
                    "left": comp["left"],
                    "right": comp["right"],
                }
            
            if original.get("fence_index"):
                fi = original["fence_index"]
                # Fence index to fence pattern (3x3 mini-grid)
                # F0=one border, F1=two adjacent, F2=three, F3=four, F4=two opposite, F7=no borders
                fence_patterns = {
                    1: "one",       # F0
                    2: "two_adj",   # F1
                    3: "three",     # F2
                    4: "four",      # F3
                    6: "two_opp",   # F4
                    5: "none",      # F7
                }
                if fi in fence_patterns:
                    cell["fence_pattern"] = fence_patterns[fi]
            
            if original.get("shape_index") is not None:
                si = original["shape_index"]
                if si < len(shape_pool):
                    cell["shape_pattern"] = shape_pool[si]
            
            json_cells.append(cell)
    
    # Generate outer_boundaries
    outer_boundaries = []
    for c in range(W):
        outer_boundaries.append({"r1": -1, "c1": c, "r2": 0, "c2": c})
        outer_boundaries.append({"r1": H - 1, "c1": c, "r2": H, "c2": c})
    for r in range(H):
        outer_boundaries.append({"r1": r, "c1": -1, "r2": r, "c2": 0})
        outer_boundaries.append({"r1": r, "c1": W - 1, "r2": r, "c2": W})
    
    puzzle = {
        "version": "1.0",
        "grid": {"height": H, "width": W},
        "cells": json_cells,
        "edges": edges,
        "vertices": vertices,
        "outer_boundaries": outer_boundaries,
        "rules": rules,
        "_meta": {
            "game_difficulty": config["difficulty"],
            "puzzle_version": config["puzzle_version"],
        }
    }
    
    return puzzle


# ─── Main ───────────────────────────────────────────────────

def convert_file(input_path, output_path=None):
    with open(input_path, "r", encoding="utf-8") as f:
        text = f.read()
    
    config, puzzle_lines, solution_lines = parse_puz(text)
    
    if config["dimensions"] is None:
        print(f"  ERROR: No DIMENSIONS in {input_path}")
        return False
    
    cells, edges, vertices = parse_puzzle_grid(config, puzzle_lines)
    puzzle = build_json(config, cells, edges, vertices)
    
    if output_path is None:
        output_path = input_path.replace(".puz", ".json")
    
    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(puzzle, f, indent=2, ensure_ascii=False)
    
    H, W = config["dimensions"]
    blocked = sum(1 for c in cells if c.get("blocked"))
    print(f"  {os.path.basename(input_path)}: {W}x{H}, {len(cells)} cells ({blocked} blocked), {len(edges)} edges, {len(config['shapes'])} shapes → {output_path}")
    return True


def batch_convert(input_dir, output_dir):
    puz_files = glob.glob(os.path.join(input_dir, "**", "*.puz"), recursive=True)
    print(f"Found {len(puz_files)} .puz files")
    
    success = 0
    for puz_path in sorted(puz_files):
        rel_path = os.path.relpath(puz_path, input_dir)
        out_path = os.path.join(output_dir, rel_path.replace(".puz", ".json"))
        if convert_file(puz_path, out_path):
            success += 1
    
    print(f"\nConverted {success}/{len(puz_files)} files")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python puz2json.py <input.puz> [output.json]")
        print("       python puz2json.py --batch <puzzles_dir> <output_dir>")
        sys.exit(1)
    
    if sys.argv[1] == "--batch":
        batch_convert(sys.argv[2], sys.argv[3])
    else:
        output = sys.argv[2] if len(sys.argv) > 2 else None
        convert_file(sys.argv[1], output)
