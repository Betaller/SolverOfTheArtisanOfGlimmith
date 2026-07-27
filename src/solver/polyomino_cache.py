"""预计算多联骨牌形状库加载器。

首次加载 data/polyominoes.json，缓存为 {size: [Shape, ...]}。
每个 Shape 的所有旋转/翻转变换也在加载时预计算，避免运行时重复。
"""

from __future__ import annotations

import json
import os
from functools import lru_cache

from src.models.board import Shape
from src.solver.shapes import all_transformations

_SHAPE_CACHE: dict[int, list[Shape]] | None = None
_TRANSFORM_CACHE: dict[int, list[set[frozenset[tuple[int, int]]]]] = {}


def _data_path() -> str:
    return os.path.join(os.path.dirname(__file__), "..", "..", "data", "polyominoes.json")


def _load() -> dict[int, list[Shape]]:
    global _SHAPE_CACHE
    if _SHAPE_CACHE is not None:
        return _SHAPE_CACHE

    path = _data_path()
    if not os.path.exists(path):
        _SHAPE_CACHE = {}
        return _SHAPE_CACHE

    with open(path, encoding="utf-8") as f:
        raw: dict[str, list[list[list[int]]]] = json.load(f)

    _SHAPE_CACHE = {}
    for size_str, shape_list in raw.items():
        size = int(size_str)
        _SHAPE_CACHE[size] = [
            Shape(cells=frozenset((r, c) for r, c in cells))
            for cells in shape_list
        ]
    return _SHAPE_CACHE


def shapes_of_size(size: int) -> list[Shape]:
    """Return all free polyominoes of the given size."""
    return _load().get(size, [])


def all_transformations_of_size(size: int) -> list[list[frozenset[tuple[int, int]]]]:
    """Pre-computed all transformations for all shapes of given size.

    Returns: list of lists, where [i][j] is the j-th transformation of shape i.
    """
    if size in _TRANSFORM_CACHE:
        return _TRANSFORM_CACHE[size]

    shapes = shapes_of_size(size)
    result: list[list[frozenset[tuple[int, int]]]] = []
    for s in shapes:
        transforms = all_transformations(s.cells)
        result.append(transforms)

    _TRANSFORM_CACHE[size] = result
    return result


def has_data() -> bool:
    return os.path.exists(_data_path())
