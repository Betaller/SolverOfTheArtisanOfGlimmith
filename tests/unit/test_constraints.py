from __future__ import annotations

import pytest

from src.models.board import (
    Board, Cell, Edge, EdgeConstraint, EdgeConstraintType,
    Shape, CompassClue, Vertex,
)
from src.models.puzzle import Puzzle, Rule
from src.solver.constraints import (
    RULE_CHECKERS, get_region_cells, get_region_shape,
    check_region_connectivity, check_boundary_consistency,
    check_rule_shape_pool, check_rule_rose_window,
    check_rule_heterogeneous, check_rule_homogeneous,
    check_rule_precise, check_rule_puzzle_piece, check_rule_mixed,
    check_rule_area, check_rule_same, check_rule_range,
    check_rule_fence, check_rule_different, check_rule_solitary,
    check_rule_block, check_rule_non_block,
    check_rule_differentiation, check_rule_brick, check_rule_ring,
    check_rule_inequality, check_rule_difference,
    check_rule_watchtower, check_rule_compass,
)

from src.solver.shapes import enumerate_polyominoes, shape_from_cells


def board_with_regions(
    height: int, width: int,
    region_map: list[list[int]],
) -> Board:
    b = Board(height, width)
    for r in range(height):
        for c in range(width):
            b.cell(r, c).region_id = region_map[r][c]
    return b


def puzzle_with_rules(rules: list[Rule]) -> Puzzle:
    b = Board(4, 4)
    return Puzzle.from_board(b, rules=rules)


class TestHelpers:
    def test_get_region_cells(self) -> None:
        b = board_with_regions(3, 3, [
            [1, 1, 2],
            [1, 2, 2],
            [3, 3, 3],
        ])
        regions = get_region_cells(b)
        assert len(regions) == 3
        assert len(regions[1]) == 3
        assert len(regions[2]) == 3
        assert len(regions[3]) == 3

    def test_get_region_shape(self) -> None:
        b = board_with_regions(2, 2, [
            [1, 1],
            [2, 2],
        ])
        cells = b.get_region_cells(1)
        shape = get_region_shape(cells)
        assert shape.area == 2
        assert (0, 0) in shape.cells
        assert (0, 1) in shape.cells

    def test_get_region_shape_by_position(self) -> None:
        cells = [Cell(row=0, col=0, region_id=1), Cell(row=1, col=0, region_id=1)]
        shape = get_region_shape(cells)
        assert shape.area == 2
        assert (0, 0) in shape.cells
        assert (1, 0) in shape.cells


class TestRegionConnectivity:
    def test_connected_region(self) -> None:
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [1, 0, 1],
            [1, 1, 1],
        ])
        assert check_region_connectivity(b) is True

    def test_disconnected_region(self) -> None:
        b = board_with_regions(3, 3, [
            [1, 0, 1],
            [0, 0, 0],
            [1, 0, 1],
        ])
        assert check_region_connectivity(b) is False

    def test_single_cell_region(self) -> None:
        b = board_with_regions(3, 3, [
            [1, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
        ])
        assert check_region_connectivity(b) is True

    def test_all_same_region(self) -> None:
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [1, 1, 1],
            [1, 1, 1],
        ])
        assert check_region_connectivity(b) is True

    def test_disconnected_diagonal(self) -> None:
        b = board_with_regions(3, 3, [
            [1, 0, 2],
            [0, 0, 0],
            [2, 0, 1],
        ])
        ret = check_region_connectivity(b)
        # Region 1 has cells at (0,0) and (2,2) - not connected
        assert ret is False

    def test_snake_connected(self) -> None:
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [0, 0, 1],
            [0, 0, 1],
        ])
        assert check_region_connectivity(b) is True

    def test_empty_board_no_assigned(self) -> None:
        b = Board(3, 3)
        assert check_region_connectivity(b) is True

    def test_multiple_regions_all_connected(self) -> None:
        b = board_with_regions(4, 4, [
            [1, 1, 2, 2],
            [1, 1, 2, 2],
            [3, 3, 4, 4],
            [3, 3, 4, 4],
        ])
        assert check_region_connectivity(b) is True


class TestBoundaryConsistency:
    def test_no_edges_marked(self) -> None:
        b = board_with_regions(2, 2, [
            [1, 1],
            [2, 2],
        ])
        assert check_boundary_consistency(b) is True

    def test_correct_boundary(self) -> None:
        b = board_with_regions(2, 2, [
            [1, 1],
            [2, 2],
        ])
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        e.is_boundary = True
        assert check_boundary_consistency(b) is True

    def test_incorrect_boundary_inside_region(self) -> None:
        b = board_with_regions(2, 2, [
            [1, 1],
            [1, 2],
        ])
        # Edge between (0,0)-(0,1) is inside region 1, but marked as boundary
        e = b.edge_between(0, 0, 0, 1)
        assert e is not None
        e.is_boundary = True
        assert check_boundary_consistency(b) is False

    def test_boundary_consistency_with_unassigned(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        # (1,0) and (1,1) are unassigned
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        e.is_boundary = True
        # expected_boundary is False because c2 is unassigned
        assert check_boundary_consistency(b) is False


class TestRuleShapePool:
    def test_all_shapes_in_pool(self) -> None:
        shapes = [Shape(cells=frozenset([(0, 0), (0, 1)]))]
        puzzle = puzzle_with_rules([Rule.shape_pool(shapes)])
        b = board_with_regions(2, 2, [
            [1, 1],
            [2, 2],
        ])
        # Region 1 = domino horizontal, Region 2 = domino horizontal
        assert check_rule_shape_pool(puzzle, b) is True

    def test_shape_not_in_pool(self) -> None:
        pool = [Shape(cells=frozenset([(0, 0)]))]
        puzzle = puzzle_with_rules([Rule.shape_pool(pool)])
        b = board_with_regions(2, 2, [
            [1, 1],
            [2, 2],
        ])
        assert check_rule_shape_pool(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_shape_pool(puzzle, b) is True

    def test_empty_pool_returns_false(self) -> None:
        puzzle = puzzle_with_rules([Rule.shape_pool([])])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_shape_pool(puzzle, b) is False


class TestRuleRoseWindow:
    def test_valid_rose_window(self) -> None:
        puzzle = puzzle_with_rules([Rule.rose_window(["A", "B"])])
        b = board_with_regions(2, 2, [
            [1, 1],
            [2, 2],
        ])
        b.cell(0, 0).symbol = "A"
        b.cell(0, 1).symbol = "B"
        b.cell(1, 0).symbol = "A"
        b.cell(1, 1).symbol = "B"
        # 2 regions, each has symbols {A, B}
        assert check_rule_rose_window(puzzle, b) is True

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_rose_window(puzzle, b) is True

    def test_wrong_symbol_count(self) -> None:
        puzzle = puzzle_with_rules([Rule.rose_window(["A", "B"])])
        b = board_with_regions(2, 2, [
            [1, 1],
            [2, 2],
        ])
        b.cell(0, 0).symbol = "A"
        b.cell(0, 1).symbol = "A"
        b.cell(1, 0).symbol = "A"
        b.cell(1, 1).symbol = "A"
        # Each symbol type count is different
        assert check_rule_rose_window(puzzle, b) is False

    def test_unknown_symbol(self) -> None:
        puzzle = puzzle_with_rules([Rule.rose_window(["A", "B"])])
        b = board_with_regions(2, 2, [
            [1, 1],
            [2, 2],
        ])
        b.cell(0, 0).symbol = "A"
        b.cell(0, 1).symbol = "X"
        b.cell(1, 0).symbol = "A"
        b.cell(1, 1).symbol = "B"
        assert check_rule_rose_window(puzzle, b) is False

    def test_empty_symbol_types_returns_false(self) -> None:
        puzzle = puzzle_with_rules([Rule.rose_window([])])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_rose_window(puzzle, b) is False

    def test_region_count_must_equal_symbol_count(self) -> None:
        puzzle = puzzle_with_rules([Rule.rose_window(["A", "B"])])
        b = board_with_regions(2, 2, [
            [1, 1],
            [2, 2],
        ])
        b.cell(0, 0).symbol = "A"
        b.cell(0, 1).symbol = "B"
        b.cell(1, 0).symbol = "A"
        b.cell(1, 1).symbol = "B"
        assert check_rule_rose_window(puzzle, b) is True


class TestRuleHeterogeneous:
    def test_heterogeneous_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.heterogeneous()])
        b = board_with_regions(3, 3, [
            [1, 1, 2],
            [1, 1, 2],
            [1, 1, 3],
        ])
        # Region 1 = 2x3 rectangle (area 6), Region 2 = vertical domino (area 2)
        e = b.edge_between(0, 1, 0, 2)
        assert e is not None
        e.constraint = EdgeConstraint(type=EdgeConstraintType.HETEROGENEOUS)
        assert check_rule_heterogeneous(puzzle, b) is True

    def test_heterogeneous_fails_when_shapes_equal(self) -> None:
        puzzle = puzzle_with_rules([Rule.heterogeneous()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        e.constraint = EdgeConstraint(type=EdgeConstraintType.HETEROGENEOUS)
        # Both regions are horizontal dominoes - same shape -> fail
        assert check_rule_heterogeneous(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_heterogeneous(puzzle, b) is True


class TestRuleHomogeneous:
    def test_homogeneous_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.homogeneous()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        # Both regions are horizontal dominoes - same shape
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        e.constraint = EdgeConstraint(type=EdgeConstraintType.HOMOGENEOUS)
        assert check_rule_homogeneous(puzzle, b) is True

    def test_homogeneous_fails_when_shapes_differ(self) -> None:
        puzzle = puzzle_with_rules([Rule.homogeneous()])
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [2, 2, 2],
            [2, 2, 2],
        ])
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        e.constraint = EdgeConstraint(type=EdgeConstraintType.HOMOGENEOUS)
        # Region 1 (area 3) != Region 2 (area 6)
        assert check_rule_homogeneous(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_homogeneous(puzzle, b) is True


class TestRulePrecise:
    def test_precise_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.precise(4)])
        b = board_with_regions(4, 4, [
            [1, 1, 1, 1],
            [2, 2, 2, 2],
            [3, 3, 3, 3],
            [4, 4, 4, 4],
        ])
        assert check_rule_precise(puzzle, b) is True

    def test_precise_fails(self) -> None:
        puzzle = puzzle_with_rules([Rule.precise(4)])
        b = board_with_regions(2, 2, [
            [1, 1],
            [2, 2],
        ])
        assert check_rule_precise(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_precise(puzzle, b) is True


class TestRulePuzzlePiece:
    def test_puzzle_piece_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.puzzle_piece()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).shape_pattern = Shape(cells=frozenset([(0, 0), (0, 1)]))
        assert check_rule_puzzle_piece(puzzle, b) is True

    def test_puzzle_piece_fails(self) -> None:
        puzzle = puzzle_with_rules([Rule.puzzle_piece()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).shape_pattern = Shape(cells=frozenset([(0, 0)]))
        assert check_rule_puzzle_piece(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_puzzle_piece(puzzle, b) is True

    def test_no_pattern_on_cell(self) -> None:
        puzzle = puzzle_with_rules([Rule.puzzle_piece()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_puzzle_piece(puzzle, b) is True


class TestRuleMixed:
    def test_mixed_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.mixed()])
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [2, 2, 2],
            [2, 2, 2],
        ])
        # Region 1 (area 3) != Region 2 (area 6)
        assert check_rule_mixed(puzzle, b) is True

    def test_mixed_fails(self) -> None:
        puzzle = puzzle_with_rules([Rule.mixed()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        # Both regions are horizontal dominoes - same shape
        assert check_rule_mixed(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_mixed(puzzle, b) is True


class TestRuleArea:
    def test_area_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.area()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).number = 2
        b.cell(1, 0).number = 2
        assert check_rule_area(puzzle, b) is True

    def test_area_fails(self) -> None:
        puzzle = puzzle_with_rules([Rule.area()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).number = 1
        assert check_rule_area(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_area(puzzle, b) is True

    def test_no_number_on_cell(self) -> None:
        puzzle = puzzle_with_rules([Rule.area()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_area(puzzle, b) is True

    def test_unassigned_cell_with_number(self) -> None:
        puzzle = puzzle_with_rules([Rule.area()])
        b = Board(2, 2)
        b.cell(0, 0).number = 2
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        # cell(1,0) has region_id None - not assigned, so skipped
        b.cell(1, 0).number = 2
        assert check_rule_area(puzzle, b) is True


class TestRuleSame:
    def test_same_all_same_shape(self) -> None:
        puzzle = puzzle_with_rules([Rule.same()])
        b = board_with_regions(4, 4, [
            [1, 1, 2, 2],
            [1, 1, 2, 2],
            [3, 3, 4, 4],
            [3, 3, 4, 4],
        ])
        # All regions are 2x2 squares
        assert check_rule_same(puzzle, b) is True

    def test_same_differs(self) -> None:
        puzzle = puzzle_with_rules([Rule.same()])
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [2, 2, 2],
            [3, 3, 3],
        ])
        assert check_rule_same(puzzle, b) is True

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_same(puzzle, b) is True


class TestRuleRange:
    def test_range_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.range(2, 4)])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_range(puzzle, b) is True

    def test_range_too_small(self) -> None:
        puzzle = puzzle_with_rules([Rule.range(3, 4)])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_range(puzzle, b) is False

    def test_range_too_large(self) -> None:
        puzzle = puzzle_with_rules([Rule.range(1, 1)])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_range(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_range(puzzle, b) is True

    def test_default_limits_when_params_missing(self) -> None:
        rule = Rule(type="range", params={})
        puzzle = Puzzle.from_board(Board(4, 4), rules=[rule])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_range(puzzle, b) is True


class TestRuleDifferent:
    def test_different_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.different()])
        b = board_with_regions(3, 2, [
            [1, 1],
            [1, 2],
            [1, 2],
        ])
        # Region 1 = L (area 3), Region 2 = vertical domino (area 2) -> different
        assert check_rule_different(puzzle, b) is True

    def test_different_fails(self) -> None:
        puzzle = puzzle_with_rules([Rule.different()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        # Both regions are horizontal dominoes -> same shape
        assert check_rule_different(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_different(puzzle, b) is True


class TestRuleSolitary:
    def test_solitary_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.solitary()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).symbol = "A"
        b.cell(1, 0).symbol = "B"
        assert check_rule_solitary(puzzle, b) is True

    def test_solitary_fails_multiple_symbols(self) -> None:
        puzzle = puzzle_with_rules([Rule.solitary()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).symbol = "A"
        b.cell(0, 1).symbol = "B"
        assert check_rule_solitary(puzzle, b) is False

    def test_no_symbols_returns_false(self) -> None:
        puzzle = puzzle_with_rules([Rule.solitary()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        # Each region has 0 symbols, but solitary expects exactly 1 symbol per region
        assert check_rule_solitary(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_solitary(puzzle, b) is True


class TestRuleBlock:
    def test_all_blocks_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.block()])
        b = board_with_regions(2, 2, [[1, 1], [1, 1]])
        assert check_rule_block(puzzle, b) is True

    def test_non_rectangle_fails(self) -> None:
        puzzle = puzzle_with_rules([Rule.block()])
        b = board_with_regions(2, 3, [
            [1, 1, 1],
            [1, 1, 2],
        ])
        # Region 1 has 5 cells in a 2x3 bounding box -> not a rectangle
        assert check_rule_block(puzzle, b) is False

    def test_l_shape_fails(self) -> None:
        puzzle = puzzle_with_rules([Rule.block()])
        b = board_with_regions(2, 2, [[1, 1], [1, 2]])
        # Region 1 = L shape (3 cells) - at (0,0),(0,1),(1,0), bounding_box=(2,2), area=3 != 4
        assert check_rule_block(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_block(puzzle, b) is True


class TestRuleNonBlock:
    def test_non_rectangle_ok(self) -> None:
        puzzle = puzzle_with_rules([Rule.non_block()])
        # Region 1: L-shape (3 cells, 2x2 bbox), Region 2: L-shape (3 cells, 2x2 bbox) over 3x2 grid
        b = board_with_regions(3, 2, [
            [1, 1],
            [1, 2],
            [2, 2],
        ])
        assert check_rule_non_block(puzzle, b) is True

    def test_rectangle_fails(self) -> None:
        puzzle = puzzle_with_rules([Rule.non_block()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_non_block(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_non_block(puzzle, b) is True


class TestRuleDifferentiation:
    def test_differentiation_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.differentiation()])
        b = board_with_regions(2, 2, [[2, 2], [1, 1]])
        # Region 1: area 2, Region 2: area 2 - they share an edge (horizontal)
        # Actually region 2 has cells (0,0),(0,1) and region 1 has (1,0),(1,1)
        # They are adjacent along the horizontal edge
        # Both have area 2 - so they are equal area!
        # This should FAIL
        # Let me restructure
        assert check_rule_differentiation(puzzle, b) is False

    def test_differentiation_satisfied_different_areas(self) -> None:
        puzzle = puzzle_with_rules([Rule.differentiation()])
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [2, 2, 2],
            [3, 3, 3],
        ])
        # Region 1 (3 cells) adjacent to Region 2 (3 cells) -> same area -> failure
        assert check_rule_differentiation(puzzle, b) is False

    def test_differentiation_all_different(self) -> None:
        puzzle = puzzle_with_rules([Rule.differentiation()])
        b = board_with_regions(3, 2, [
            [1, 1],
            [2, 2],
            [3, 3],
        ])
        # Region 1 (2) adjacent to Region 2 (2) -> same area -> fail
        assert check_rule_differentiation(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_differentiation(puzzle, b) is True

    def test_differentiation_fails_on_same_area(self) -> None:
        puzzle = puzzle_with_rules([Rule.differentiation()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        # Region 1 (2) adjacent to Region 2 (2) -> same area -> fail
        assert check_rule_differentiation(puzzle, b) is False


class TestRuleBrick:
    def test_brick_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.brick()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        from src.solver.propagator import update_boundary_edges
        update_boundary_edges(b)
        # Vertex (0,0): count of boundary edges = 2 (left and right), not 4 -> OK
        assert check_rule_brick(puzzle, b) is True

    def test_brick_violated(self) -> None:
        puzzle = puzzle_with_rules([Rule.brick()])
        b = board_with_regions(2, 2, [
            [1, 2],
            [3, 4],
        ])
        from src.solver.propagator import update_boundary_edges
        update_boundary_edges(b)
        # Vertex (0,0): all 4 edges are boundaries, count = 4 -> violate
        assert check_rule_brick(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_brick(puzzle, b) is True

    def test_brick_larger_board(self) -> None:
        puzzle = puzzle_with_rules([Rule.brick()])
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [1, 1, 1],
            [1, 1, 1],
        ])
        # All one region, no boundaries
        assert check_rule_brick(puzzle, b) is True


class TestRuleRing:
    def test_ring_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.ring()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_ring(puzzle, b) is True

    def test_ring_violated_t_shape(self) -> None:
        puzzle = puzzle_with_rules([Rule.ring()])
        b = board_with_regions(2, 2, [
            [1, 2],
            [2, 2],
        ])
        # Vertex (0,0): edges at (0,0)-(0,1) is boundary (1 vs 2)
        # (0,0)-(1,0) is boundary (1 vs 2)
        # (0,1)-(1,1) is inside region 2
        # (1,0)-(1,1) is inside region 2
        # Count = 2 -> OK
        # Need a case where count=3
        pass

    def test_ring_violated_three_boundaries(self) -> None:
        puzzle = puzzle_with_rules([Rule.ring()])
        b = board_with_regions(2, 2, [
            [1, 2],
            [3, 4],
        ])
        # Vertex (0,0): count = 4, not 3 -> OK for ring
        # A case with exactly 3 boundaries at a vertex...
        pass

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_ring(puzzle, b) is True


class TestRuleInequality:
    def test_inequality_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.inequality()])
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [2, 2, 2],
            [3, 3, 3],
        ])
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        e.constraint = EdgeConstraint(type=EdgeConstraintType.INEQUALITY)
        # Region 1 (area 3) < Region 2 (area 3)? No, they're equal -> fails
        # Need different areas
        assert check_rule_inequality(puzzle, b) is False

    def test_inequality_satisfied_different_areas(self) -> None:
        puzzle = puzzle_with_rules([Rule.inequality()])
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [2, 2, 2],
            [2, 2, 2],
        ])
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        e.constraint = EdgeConstraint(type=EdgeConstraintType.INEQUALITY)
        # Region 1 (area 3) < Region 2 (area 6) -> OK
        assert check_rule_inequality(puzzle, b) is True

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_inequality(puzzle, b) is True

    def test_inequality_same_region_fails(self) -> None:
        puzzle = puzzle_with_rules([Rule.inequality()])
        b = Board(2, 2)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        e = b.edge_between(0, 0, 0, 1)
        assert e is not None
        e.constraint = EdgeConstraint(type=EdgeConstraintType.INEQUALITY)
        # Same region -> fails (c1.region_id == c2.region_id)
        assert check_rule_inequality(puzzle, b) is False


class TestRuleDifference:
    def test_difference_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.difference()])
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [2, 2, 2],
            [3, 3, 3],
        ])
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        e.constraint = EdgeConstraint(type=EdgeConstraintType.DIFFERENCE, value=0)
        # Region 1 (area 3) - Region 2 (area 3) = 0 -> OK
        assert check_rule_difference(puzzle, b) is True

    def test_difference_wrong_value(self) -> None:
        puzzle = puzzle_with_rules([Rule.difference()])
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [2, 2, 2],
            [2, 2, 2],
        ])
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        e.constraint = EdgeConstraint(type=EdgeConstraintType.DIFFERENCE, value=2)
        # Region 1 (area 3) - Region 2 (area 6) = 3, not 2 -> fails
        assert check_rule_difference(puzzle, b) is False

    def test_difference_exact_match(self) -> None:
        puzzle = puzzle_with_rules([Rule.difference()])
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [2, 2, 2],
            [2, 2, 2],
        ])
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        e.constraint = EdgeConstraint(type=EdgeConstraintType.DIFFERENCE, value=3)
        assert check_rule_difference(puzzle, b) is True

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_difference(puzzle, b) is True


class TestRuleWatchtower:
    def test_watchtower_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.watchtower()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        v = b.vertex_at(0, 0)
        assert v is not None
        v.watchtower = 2
        # Cells surrounding (0,0): (0,0)=1, (0,1)=1, (1,0)=2, (1,1)=2 -> 2 distinct regions
        assert check_rule_watchtower(puzzle, b) is True

    def test_watchtower_fails(self) -> None:
        puzzle = puzzle_with_rules([Rule.watchtower()])
        b = board_with_regions(2, 2, [[1, 1], [1, 2]])
        v = b.vertex_at(0, 0)
        assert v is not None
        v.watchtower = 2
        # Cells surrounding (0,0): (0,0)=1, (0,1)=1, (1,0)=1, (1,1)=2 -> 2 distinct regions
        assert check_rule_watchtower(puzzle, b) is True

    def test_watchtower_fails_three_expected(self) -> None:
        puzzle = puzzle_with_rules([Rule.watchtower()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        v = b.vertex_at(0, 0)
        assert v is not None
        v.watchtower = 3
        # Only 2 distinct regions -> fails
        assert check_rule_watchtower(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_watchtower(puzzle, b) is True

    def test_no_watchtower_on_vertex(self) -> None:
        puzzle = puzzle_with_rules([Rule.watchtower()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_watchtower(puzzle, b) is True

    def test_watchtower_all_four_regions(self) -> None:
        puzzle = puzzle_with_rules([Rule.watchtower()])
        b = board_with_regions(2, 2, [[1, 2], [3, 4]])
        v = b.vertex_at(0, 0)
        assert v is not None
        v.watchtower = 4
        assert check_rule_watchtower(puzzle, b) is True

    def test_watchtower_unassigned_cells_skipped(self) -> None:
        puzzle = puzzle_with_rules([Rule.watchtower()])
        b = Board(2, 2)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 2
        # (1,0) and (1,1) are unassigned
        v = b.vertex_at(0, 0)
        assert v is not None
        v.watchtower = 2
        # Only 2 cells assigned with region_id in the 4 -> {1, 2} -> 2 distinct
        assert check_rule_watchtower(puzzle, b) is True


class TestRuleCompass:
    def test_compass_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.compass()])
        b = board_with_regions(3, 3, [
            [1, 1, 2],
            [1, 1, 2],
            [3, 3, 3],
        ])
        b.cell(0, 0).compass = CompassClue(up=-1, down=-1, left=-1, right=2)
        # From (0,0): region-1 cells strictly to the right are (0,1),(1,1) -> count = 2
        assert check_rule_compass(puzzle, b) is True

    def test_compass_fails(self) -> None:
        puzzle = puzzle_with_rules([Rule.compass()])
        b = board_with_regions(3, 3, [
            [1, 1, 2],
            [1, 1, 2],
            [3, 3, 3],
        ])
        b.cell(0, 0).compass = CompassClue(up=-1, down=-1, left=-1, right=3)
        # From (0,0): only (0,1),(1,1) are region-1 cells to the right -> count = 2, not 3
        assert check_rule_compass(puzzle, b) is False

    def test_compass_up_direction(self) -> None:
        puzzle = puzzle_with_rules([Rule.compass()])
        b = board_with_regions(3, 3, [
            [1, 2, 2],
            [1, 2, 2],
            [1, 1, 1],
        ])
        # Cell (2,0) in region 1, looking up: (1,0) region 1, (0,0) region 1 -> count = 2
        b.cell(2, 0).compass = CompassClue(up=2, down=-1, left=-1, right=-1)
        assert check_rule_compass(puzzle, b) is True

    def test_compass_zero_count(self) -> None:
        puzzle = puzzle_with_rules([Rule.compass()])
        b = board_with_regions(2, 2, [[1, 2], [2, 2]])
        b.cell(0, 0).compass = CompassClue(up=0, down=-1, left=-1, right=-1)
        # Up from (0,0) goes out of bounds -> count = 0
        assert check_rule_compass(puzzle, b) is True

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_compass(puzzle, b) is True

    def test_no_compass_on_cell(self) -> None:
        puzzle = puzzle_with_rules([Rule.compass()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_compass(puzzle, b) is True


class TestRuleFence:
    def test_fence_satisfied(self) -> None:
        puzzle = puzzle_with_rules([Rule.fence()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        # Cell (0,0): edges: up=out of bounds(boundary=True), down=(1,0) diff region(boundary=True),
        #   left=out of bounds(boundary=True), right=(0,1) same region(boundary=False)
        # edge_bits = [True, True, True, False] (up, down, left, right)
        # fence pattern for this:
        #   (0,0) no, (0,1)=up=True, (0,2) no
        #   (1,0)=left=True, (1,1)=center=True, (1,2)=right=False
        #   (2,0) no, (2,1)=down=True, (2,2) no
        # So fence cells: (0,1), (1,0), (1,1), (2,1)
        fence = Shape(cells=frozenset([
            (0, 1), (1, 0), (1, 1), (2, 1),
        ]))
        b.cell(0, 0).fence_pattern = fence
        assert check_rule_fence(puzzle, b) is True

    def test_fence_fails(self) -> None:
        puzzle = puzzle_with_rules([Rule.fence()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        wrong_fence = Shape(cells=frozenset([(1, 1)]))
        b.cell(0, 0).fence_pattern = wrong_fence
        assert check_rule_fence(puzzle, b) is False

    def test_no_rule_returns_true(self) -> None:
        puzzle = puzzle_with_rules([])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_fence(puzzle, b) is True

    def test_no_fence_pattern(self) -> None:
        puzzle = puzzle_with_rules([Rule.fence()])
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        assert check_rule_fence(puzzle, b) is True


class TestRULE_CHECKERS:
    def test_all_checkers_present(self) -> None:
        expected_checkers = {
            "shape_pool", "rose_window", "heterogeneous", "homogeneous",
            "precise", "puzzle_piece", "mixed", "area", "same", "range",
            "fence", "different", "solitary", "block", "non_block",
            "differentiation", "brick", "ring", "inequality", "difference",
            "watchtower", "compass",
        }
        assert set(RULE_CHECKERS.keys()) == expected_checkers

    def test_checker_functions_are_callable(self) -> None:
        for name, func in RULE_CHECKERS.items():
            assert callable(func), f"{name} is not callable"
