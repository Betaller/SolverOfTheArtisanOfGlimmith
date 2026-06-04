from __future__ import annotations

import pytest

from src.models.board import Shape
from src.solver.shapes import (
    normalize, canonical_key, shapes_equal, match_shape_pool,
    is_rectangle, enumerate_polyominoes, all_transformations,
    shape_key, shape_from_cells, shape_bitmap, shape_from_bitmap,
)


class TestNormalize:
    def test_normalize_single_cell(self) -> None:
        result = normalize(frozenset([(5, 7)]))
        assert result == frozenset([(0, 0)])

    def test_normalize_already_normalized(self) -> None:
        result = normalize(frozenset([(0, 0), (0, 1), (1, 0)]))
        assert result == frozenset([(0, 0), (0, 1), (1, 0)])

    def test_normalize_moves_to_origin(self) -> None:
        result = normalize(frozenset([(2, 3), (2, 4), (3, 3)]))
        assert result == frozenset([(0, 0), (0, 1), (1, 0)])

    def test_normalize_negative_coords(self) -> None:
        result = normalize(frozenset([(-1, -1), (-1, 0), (0, -1)]))
        assert result == frozenset([(0, 0), (0, 1), (1, 0)])

    def test_normalize_empty(self) -> None:
        result = normalize(frozenset())
        assert result == frozenset()


class TestTransformations:
    def test_rotate_90(self) -> None:
        from src.solver.shapes import rotate_90
        cells = frozenset([(0, 0), (0, 1)])
        rotated = rotate_90(cells)
        assert (0, 0) in rotated
        assert (1, 0) in rotated
        assert len(rotated) == 2

    def test_rotate_180(self) -> None:
        from src.solver.shapes import rotate_180
        cells = frozenset([(0, 0), (0, 1)])
        rotated = rotate_180(cells)
        assert (0, 0) in rotated
        assert (0, -1) in rotated
        assert len(rotated) == 2

    def test_flip_horizontal(self) -> None:
        from src.solver.shapes import flip_horizontal
        cells = frozenset([(0, 0), (0, 1)])
        flipped = flip_horizontal(cells)
        assert (0, 0) in flipped
        assert (0, -1) in flipped

    def test_flip_vertical(self) -> None:
        from src.solver.shapes import flip_vertical
        cells = frozenset([(0, 0), (1, 0)])
        flipped = flip_vertical(cells)
        assert (0, 0) in flipped
        assert (-1, 0) in flipped

    def test_all_transformations_domino(self) -> None:
        cells = frozenset([(0, 0), (0, 1)])
        transforms = all_transformations(cells)
        # 4 rotations * 2 flips = 8, but some may be duplicates
        assert len(transforms) == 8
        for t in transforms:
            assert len(t) == 2
            assert min(r for r, _ in t) == 0
            assert min(c for _, c in t) == 0

    def test_all_transformations_single_cell(self) -> None:
        cells = frozenset([(0, 0)])
        transforms = all_transformations(cells)
        # All 8 transforms should be the same normalized form
        for t in transforms:
            assert t == frozenset([(0, 0)])

    def test_all_transformations_all_normalized(self) -> None:
        cells = frozenset([(2, 3), (2, 4)])
        transforms = all_transformations(cells)
        for t in transforms:
            min_r = min(r for r, _ in t)
            min_c = min(c for _, c in t)
            assert min_r == 0
            assert min_c == 0


class TestCanonicalKey:
    def test_canonical_key_consistency(self) -> None:
        k1 = canonical_key(frozenset([(0, 0), (0, 1)]))
        k2 = canonical_key(frozenset([(0, 0), (1, 0)]))
        assert k1 == k2

    def test_canonical_key_different_shapes(self) -> None:
        k1 = canonical_key(frozenset([(0, 0)]))
        k2 = canonical_key(frozenset([(0, 0), (0, 1)]))
        assert k1 != k2

    def test_canonical_key_shifted_same_shape(self) -> None:
        k1 = canonical_key(frozenset([(0, 0), (0, 1)]))
        k2 = canonical_key(frozenset([(5, 7), (5, 8)]))
        assert k1 == k2

    def test_canonical_key_l_shape(self) -> None:
        k1 = canonical_key(frozenset([(0, 0), (1, 0), (1, 1)]))
        k2 = canonical_key(frozenset([(0, 0), (0, 1), (1, 0)]))
        # L3 rotated 90 degrees
        assert k1 == k2


class TestShapesEqual:
    def test_equal_shapes(self) -> None:
        s1 = Shape(cells=frozenset([(0, 0), (0, 1)]))
        s2 = Shape(cells=frozenset([(5, 5), (5, 6)]))
        assert shapes_equal(s1, s2) is True

    def test_rotated_equal_shapes(self) -> None:
        s1 = Shape(cells=frozenset([(0, 0), (0, 1)]))
        s2 = Shape(cells=frozenset([(0, 0), (1, 0)]))
        assert shapes_equal(s1, s2) is True

    def test_different_shapes(self) -> None:
        s1 = Shape(cells=frozenset([(0, 0)]))
        s2 = Shape(cells=frozenset([(0, 0), (0, 1)]))
        assert shapes_equal(s1, s2) is False

    def test_same_shape_different_area_not_equal(self) -> None:
        s1 = Shape(cells=frozenset([(0, 0), (0, 1)]))
        s2 = Shape(cells=frozenset([(0, 0), (0, 1), (1, 0)]))
        assert shapes_equal(s1, s2) is False


class TestMatchShapePool:
    def test_match_exists(self) -> None:
        shape = Shape(cells=frozenset([(0, 0), (0, 1)]))
        pool = [Shape(cells=frozenset([(0, 0), (0, 1)]))]
        result = match_shape_pool(shape, pool)
        assert result is not None
        assert result.startswith("shape_")

    def test_match_not_exists(self) -> None:
        shape = Shape(cells=frozenset([(0, 0), (0, 1)]))
        pool = [Shape(cells=frozenset([(0, 0)]))]
        result = match_shape_pool(shape, pool)
        assert result is None

    def test_match_rotated(self) -> None:
        shape = Shape(cells=frozenset([(0, 0), (1, 0)]))
        pool = [Shape(cells=frozenset([(0, 0), (0, 1)]))]
        result = match_shape_pool(shape, pool)
        assert result is not None

    def test_match_empty_pool(self) -> None:
        shape = Shape(cells=frozenset([(0, 0)]))
        result = match_shape_pool(shape, [])
        assert result is None


class TestIsRectangle:
    def test_single_cell(self) -> None:
        shape = Shape(cells=frozenset([(0, 0)]))
        assert is_rectangle(shape) is True

    def test_2x1(self) -> None:
        shape = Shape(cells=frozenset([(0, 0), (0, 1)]))
        assert is_rectangle(shape) is True

    def test_2x2(self) -> None:
        shape = Shape(cells=frozenset([(0, 0), (0, 1), (1, 0), (1, 1)]))
        assert is_rectangle(shape) is True

    def test_3x2(self) -> None:
        cells = {(r, c) for r in range(3) for c in range(2)}
        shape = Shape(cells=frozenset(cells))
        assert is_rectangle(shape) is True

    def test_l_shape(self) -> None:
        shape = Shape(cells=frozenset([(0, 0), (1, 0), (1, 1)]))
        assert is_rectangle(shape) is False

    def test_t_shape(self) -> None:
        shape = Shape(cells=frozenset([(0, 0), (0, 1), (0, 2), (1, 1)]))
        assert is_rectangle(shape) is False

    def test_empty(self) -> None:
        shape = Shape(cells=frozenset())
        assert is_rectangle(shape) is False


class TestShapeFromCells:
    def test_simple(self) -> None:
        shape = shape_from_cells([(0, 0), (0, 1)])
        assert shape.area == 2
        assert (0, 0) in shape.cells
        assert (0, 1) in shape.cells

    def test_normalization(self) -> None:
        shape = shape_from_cells([(5, 5), (5, 6)])
        assert (0, 0) in shape.cells
        assert (0, 1) in shape.cells

    def test_empty_list(self) -> None:
        shape = shape_from_cells([])
        assert shape.area == 0


class TestShapeKey:
    def test_shape_key(self) -> None:
        s1 = Shape(cells=frozenset([(0, 0), (0, 1)]))
        s2 = Shape(cells=frozenset([(0, 0), (1, 0)]))
        assert shape_key(s1) == shape_key(s2)

    def test_shape_key_different(self) -> None:
        s1 = Shape(cells=frozenset([(0, 0)]))
        s2 = Shape(cells=frozenset([(0, 0), (0, 1)]))
        assert shape_key(s1) != shape_key(s2)


class TestShapeBitmap:
    def test_single_cell(self) -> None:
        shape = Shape(cells=frozenset([(0, 0)]))
        bitmap = shape_bitmap(shape)
        assert bitmap == [[True]]

    def test_domino_horizontal(self) -> None:
        shape = Shape(cells=frozenset([(0, 0), (0, 1)]))
        bitmap = shape_bitmap(shape)
        assert bitmap == [[True, True]]

    def test_domino_vertical(self) -> None:
        shape = Shape(cells=frozenset([(0, 0), (1, 0)]))
        bitmap = shape_bitmap(shape)
        assert bitmap == [[True], [True]]

    def test_l_shape(self) -> None:
        shape = Shape(cells=frozenset([(0, 0), (1, 0), (1, 1)]))
        bitmap = shape_bitmap(shape)
        assert bitmap == [[True, False], [True, True]]


class TestShapeFromBitmap:
    def test_single_true(self) -> None:
        shape = shape_from_bitmap([[True]])
        assert shape.cells == frozenset([(0, 0)])

    def test_single_false(self) -> None:
        shape = shape_from_bitmap([[False]])
        assert shape.cells == frozenset()

    def test_rectangle(self) -> None:
        bitmap = [[True, True], [True, True]]
        shape = shape_from_bitmap(bitmap)
        assert shape.cells == frozenset([(0, 0), (0, 1), (1, 0), (1, 1)])

    def test_roundtrip(self) -> None:
        original = Shape(cells=frozenset([(0, 0), (1, 0), (1, 1)]))
        bitmap = shape_bitmap(original)
        restored = shape_from_bitmap(bitmap)
        assert shapes_equal(original, restored)


class TestEnumeratePolyominoes:
    def test_n_1(self) -> None:
        shapes = enumerate_polyominoes(1)
        assert len(shapes) == 1
        assert shapes[0].cells == frozenset([(0, 0)])

    def test_n_2(self) -> None:
        shapes = enumerate_polyominoes(2)
        assert len(shapes) == 1
        assert shapes[0].area == 2

    def test_n_3(self) -> None:
        shapes = enumerate_polyominoes(3)
        assert len(shapes) == 2
        areas = [s.area for s in shapes]
        assert all(a == 3 for a in areas)

    def test_n_4(self) -> None:
        shapes = enumerate_polyominoes(4)
        assert len(shapes) == 5
        areas = [s.area for s in shapes]
        assert all(a == 4 for a in areas)

    def test_n_5(self) -> None:
        shapes = enumerate_polyominoes(5)
        assert len(shapes) == 12

    def test_n_0(self) -> None:
        shapes = enumerate_polyominoes(0)
        assert shapes == []

    def test_n_negative(self) -> None:
        shapes = enumerate_polyominoes(-1)
        assert shapes == []

    def test_no_duplicates(self) -> None:
        shapes = enumerate_polyominoes(4)
        keys = {canonical_key(s.cells) for s in shapes}
        assert len(keys) == len(shapes)

    def test_known_tetrominoes(self) -> None:
        shapes = enumerate_polyominoes(4)
        keys = {canonical_key(s.cells) for s in shapes}
        i4 = canonical_key(frozenset([(0, 0), (0, 1), (0, 2), (0, 3)]))
        o = canonical_key(frozenset([(0, 0), (0, 1), (1, 0), (1, 1)]))
        l4 = canonical_key(frozenset([(0, 0), (0, 1), (0, 2), (1, 0)]))
        t4 = canonical_key(frozenset([(0, 0), (0, 1), (0, 2), (1, 1)]))
        s4 = canonical_key(frozenset([(0, 0), (0, 1), (1, 1), (1, 2)]))
        assert i4 in keys
        assert o in keys
        assert l4 in keys
        assert t4 in keys
        assert s4 in keys

    def test_tetromino_counts(self) -> None:
        shapes = enumerate_polyominoes(4)
        assert len(shapes) == 5


class TestIntegration:
    def test_canonical_key_for_all_polyominoes_4(self) -> None:
        shapes = enumerate_polyominoes(4)
        keys = set()
        for s in shapes:
            keys.add(shape_key(s))
        assert len(keys) == 5
        for s in shapes:
            for t in all_transformations(s.cells):
                k = canonical_key(t)
                assert k in keys

    def test_shape_pool_matching(self) -> None:
        pool = enumerate_polyominoes(4)
        shape = Shape(cells=frozenset([(0, 0), (0, 1), (1, 0), (1, 1)]))
        assert match_shape_pool(shape, pool) is not None

    def test_shape_not_in_pool(self) -> None:
        pool = enumerate_polyominoes(3)
        shape = Shape(cells=frozenset([(0, 0), (0, 1), (1, 0), (1, 1)]))
        assert match_shape_pool(shape, pool) is None
