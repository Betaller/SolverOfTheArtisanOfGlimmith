from __future__ import annotations

import json
from typing import Any

from src.models.board import Board, Cell, Edge, EdgeConstraint, EdgeConstraintType, Vertex, Shape, CompassClue
from src.models.puzzle import Puzzle, Rule
from src.solver.exceptions import PuzzleFormatError


def shape_to_dict(shape: Shape) -> list[list[int]]:
    return [[r, c] for r, c in shape.cells]


def shape_from_dict(d: list[list[int]]) -> Shape:
    return Shape(cells=frozenset((r, c) for r, c in d))


def rule_to_dict(rule: Rule) -> dict[str, Any]:
    result: dict[str, Any] = {"type": rule.type}
    if rule.params:
        params = dict(rule.params)
        if "shapes" in params:
            params["shapes"] = [shape_to_dict(s) for s in params["shapes"]]
        result["params"] = params
    return result


def rule_from_dict(d: dict[str, Any]) -> Rule:
    params = dict(d.get("params", {}))
    if "shapes" in params:
        params["shapes"] = [shape_from_dict(s) for s in params["shapes"]]
    return Rule(type=d["type"], params=params)


def puzzle_to_dict(puzzle: Puzzle) -> dict[str, Any]:
    cells_data: list[dict[str, Any]] = []
    for c in puzzle.cells:
        entry: dict[str, Any] = {"row": c.row, "col": c.col}
        if c.number is not None:
            entry["number"] = c.number
        if c.symbol is not None:
            entry["symbol"] = c.symbol
        if c.shape_pattern is not None:
            entry["shape_pattern"] = shape_to_dict(c.shape_pattern)
        if c.compass is not None:
            entry["compass"] = {
                "up": c.compass.up,
                "down": c.compass.down,
                "left": c.compass.left,
                "right": c.compass.right,
            }
        if c.fence_pattern is not None:
            entry["fence_pattern"] = shape_to_dict(c.fence_pattern)
        if c.blocked:
            entry["blocked"] = True
        cells_data.append(entry)

    edges_data: list[dict[str, Any]] = []
    for e in puzzle.edges:
        entry: dict[str, Any] = {"r1": e.r1, "c1": e.c1, "r2": e.r2, "c2": e.c2}
        if e.is_boundary:
            entry["is_boundary"] = True
        if e.constraint is not None:
            cdict: dict[str, Any] = {"type": e.constraint.type.value}
            if e.constraint.value is not None:
                cdict["value"] = e.constraint.value
            entry["constraint"] = cdict
        edges_data.append(entry)

    vertices_data: list[dict[str, Any]] = []
    for v in puzzle.vertices:
        entry: dict[str, Any] = {"row": v.row, "col": v.col}
        if v.watchtower is not None:
            entry["watchtower"] = v.watchtower
        vertices_data.append(entry)

    ob_data = [{"r1": k[0], "c1": k[1], "r2": k[2], "c2": k[3]} for k in puzzle.outer_boundaries]

    return {
        "version": "1.0",
        "grid": {"height": puzzle.height, "width": puzzle.width},
        "cells": cells_data,
        "edges": edges_data,
        "vertices": vertices_data,
        "outer_boundaries": ob_data,
        "rules": [rule_to_dict(r) for r in puzzle.rules],
        "shape_pool": [shape_to_dict(s) for s in puzzle.shape_pool],
    }


def dict_to_puzzle(data: dict[str, Any]) -> Puzzle:
    try:
        grid = data["grid"]
        height = int(grid["height"])
        width = int(grid["width"])
    except (KeyError, TypeError, ValueError) as e:
        raise PuzzleFormatError(f"无效的网格尺寸: {e}")

    board = Board(height, width)

    for cdata in data.get("cells", []):
        r, c = int(cdata["row"]), int(cdata["col"])
        cell = board.cell(r, c)
        if "number" in cdata:
            cell.number = int(cdata["number"])
        if "symbol" in cdata:
            cell.symbol = str(cdata["symbol"])
        if "shape_pattern" in cdata:
            cell.shape_pattern = shape_from_dict(cdata["shape_pattern"])
        if "compass" in cdata:
            comp = cdata["compass"]
            cell.compass = CompassClue(
                up=int(comp["up"]), down=int(comp["down"]),
                left=int(comp["left"]), right=int(comp["right"]),
            )
        if "fence_pattern" in cdata:
            cell.fence_pattern = shape_from_dict(cdata["fence_pattern"])
        if cdata.get("blocked"):
            cell.blocked = True

    for edata in data.get("edges", []):
        e = board.edge_between(
            int(edata["r1"]), int(edata["c1"]),
            int(edata["r2"]), int(edata["c2"]),
        )
        if e is None:
            raise PuzzleFormatError(f"无效的边框坐标: ({edata['r1']},{edata['c1']})-({edata['r2']},{edata['c2']})")
        if edata.get("is_boundary"):
            e.is_boundary = True
        if "constraint" in edata:
            cd = edata["constraint"]
            ctype = EdgeConstraintType(cd["type"])
            e.constraint = EdgeConstraint(type=ctype, value=cd.get("value"))

    for vdata in data.get("vertices", []):
        v = board.vertex_at(int(vdata["row"]), int(vdata["col"]))
        if v is None:
            raise PuzzleFormatError(f"无效的顶点坐标: ({vdata['row']},{vdata['col']})")
        if "watchtower" in vdata:
            v.watchtower = int(vdata["watchtower"])

    rules = [rule_from_dict(r) for r in data.get("rules", [])]
    shape_pool = [shape_from_dict(s) for s in data.get("shape_pool", [])]

    outer_boundaries = []
    for ob in data.get("outer_boundaries", []):
        key = (int(ob["r1"]), int(ob["c1"]), int(ob["r2"]), int(ob["c2"]))
        outer_boundaries.append(key)

    return Puzzle(
        height=height, width=width,
        cells=board.cells(), edges=board.edges(),
        vertices=board.vertices(),
        outer_boundaries=outer_boundaries,
        rules=rules, shape_pool=shape_pool,
    )


def serialize(puzzle: Puzzle, path: str) -> None:
    data = puzzle_to_dict(puzzle)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)


def deserialize(path: str) -> Puzzle:
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)
    return dict_to_puzzle(data)
