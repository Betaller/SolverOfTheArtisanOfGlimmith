from __future__ import annotations

from src.models.board import Board, CompassClue, Shape
from src.models.puzzle import Puzzle, Rule
from src.validation.validator import IndependentValidator, _canonical_key


def _puzzle(rules, h=4, w=4) -> Puzzle:
    b = Board(h, w)
    return Puzzle(height=h, width=w, cells=b.cells(), edges=b.edges(),
                  vertices=b.vertices(), rules=rules)


def _board(h, w, regions, blocked=()) -> Board:
    b = Board(h, w)
    for r, c in blocked:
        b.cell(r, c).blocked = True
    for rid, cells in enumerate(regions):
        for r, c in cells:
            b.cell(r, c).region_id = rid
    return b


def _shape(cells):
    return Shape(cells=frozenset(cells))


class TestIndependentValidatorShapePool:
    def test_valid_tiling_passes(self) -> None:
        pool = [[[0, 0], [0, 1], [1, 0], [1, 1]]]
        puzzle = _puzzle([Rule("shape_pool", {"shapes": [pool[0]]})], h=2, w=2)
        board = _board(2, 2, [[(0, 0), (0, 1), (1, 0), (1, 1)]])
        res = IndependentValidator().validate(puzzle, board)
        assert res.solved
        assert res.errors == []

    def test_wrong_shape_rejected(self) -> None:
        pool = [[[0, 0], [0, 1], [1, 0], [1, 1]]]
        puzzle = _puzzle([Rule("shape_pool", {"shapes": [pool[0]]})], h=2, w=2)
        board = _board(2, 2, [[(0, 0), (0, 1)], [(1, 0), (1, 1)]])
        res = IndependentValidator().validate(puzzle, board)
        assert not res.solved
        assert any("形状池" in e for e in res.errors)

    def test_shape_params_may_be_shape_objects(self) -> None:
        puzzle = _puzzle([Rule("shape_pool", {"shapes": [_shape(((0, 0), (0, 1), (1, 0), (1, 1)))]})], h=2, w=2)
        board = _board(2, 2, [[(0, 0), (0, 1), (1, 0), (1, 1)]])
        res = IndependentValidator().validate(puzzle, board)
        assert res.solved


class TestIndependentValidatorCompass:
    def test_halfplane_rule(self) -> None:
        puzzle = _puzzle([Rule.compass()], h=3, w=3)
        board = _board(3, 3, [
            [(0, 0), (0, 1), (1, 0), (1, 1)],
            [(0, 2), (1, 2)],
            [(2, 0), (2, 1), (2, 2)],
        ])
        # region 0 cells to the right of (0,0): (0,1),(1,1) -> 2
        board.cell(0, 0).compass = CompassClue(up=-1, down=-1, left=-1, right=2)
        res = IndependentValidator().validate(puzzle, board)
        assert res.solved

    def test_halfplane_wrong_value_rejected(self) -> None:
        puzzle = _puzzle([Rule.compass()], h=3, w=3)
        board = _board(3, 3, [
            [(0, 0), (0, 1), (1, 0), (1, 1)],
            [(0, 2), (1, 2)],
            [(2, 0), (2, 1), (2, 2)],
        ])
        board.cell(0, 0).compass = CompassClue(up=-1, down=-1, left=-1, right=1)
        res = IndependentValidator().validate(puzzle, board)
        assert not res.solved


class TestIndependentValidatorFence:
    def test_blocked_neighbour_counts_as_boundary(self) -> None:
        puzzle = _puzzle([Rule.fence()], h=2, w=2)
        board = _board(2, 2, [[(0, 0), (0, 1)]], blocked=((1, 0), (1, 1)))
        # cell (0,0): up=out(True), down=blocked(True), left=out(True), right=same(False)
        # pattern: up+down+left -> {(1,1),(0,1),(2,1),(1,0)}
        board.cell(0, 0).fence_pattern = _shape(((1, 1), (0, 1), (2, 1), (1, 0)))
        res = IndependentValidator().validate(puzzle, board)
        assert res.solved


class TestIndependentValidatorBasics:
    def test_unassigned_cell_rejected(self) -> None:
        puzzle = _puzzle([Rule.precise(4)])
        board = _board(2, 2, [])
        res = IndependentValidator().validate(puzzle, board)
        assert not res.solved
        assert any("未分配" in e for e in res.errors)

    def test_disconnected_region_rejected(self) -> None:
        puzzle = _puzzle([], h=3, w=3)
        board = _board(3, 3, [[(0, 0), (0, 2)]])
        res = IndependentValidator().validate(puzzle, board)
        assert not res.solved
        assert any("不连通" in e for e in res.errors)

    def test_pre_drawn_boundary_same_region_rejected(self) -> None:
        puzzle = _puzzle([], h=2, w=2)
        board = _board(2, 2, [[(0, 0), (0, 1)]])
        puzzle.edges[0].is_boundary = True  # (0,0)-(0,1) same region
        res = IndependentValidator().validate(puzzle, board)
        assert not res.solved
        assert any("预画边界" in e for e in res.errors)


class TestCanonicalKey:
    def test_rotations_reflections_equal(self) -> None:
        assert _canonical_key(frozenset({(0, 0), (0, 1), (1, 0)})) == \
            _canonical_key(frozenset({(0, 0), (0, 1), (1, 1)}))
        assert _canonical_key(frozenset({(0, 0), (0, 1), (1, 0)})) != \
            _canonical_key(frozenset({(0, 0), (0, 1), (1, 2)}))
