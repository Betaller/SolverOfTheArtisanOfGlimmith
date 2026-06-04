from __future__ import annotations

import pytest

from src.models.board import Board, Shape, CompassClue
from src.models.puzzle import Puzzle, Rule
from src.solver.propagator import ConstraintPropagator, update_boundary_edges


def board_with_regions(
    height: int, width: int,
    region_map: list[list[int]],
) -> Board:
    b = Board(height, width)
    for r in range(height):
        for c in range(width):
            b.cell(r, c).region_id = region_map[r][c]
    return b


class TestConstraintPropagatorInit:
    def test_init_no_rules(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b)
        propagator = ConstraintPropagator(puzzle)
        assert propagator.puzzle is not None
        assert propagator._pool_keys is None

    def test_init_with_shape_pool(self) -> None:
        b = Board(4, 4)
        pool = [Shape(cells=frozenset([(0, 0), (0, 1)]))]
        puzzle = Puzzle.from_board(b, rules=[Rule.shape_pool(pool)])
        propagator = ConstraintPropagator(puzzle)
        assert propagator._pool_keys is not None
        assert len(propagator._pool_keys) == 1

    def test_init_with_empty_shape_pool(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.shape_pool([])])
        propagator = ConstraintPropagator(puzzle)
        assert propagator._pool_keys is not None
        assert len(propagator._pool_keys) == 0


class TestCheckRegionValid:
    def test_valid_region_no_rules(self) -> None:
        b = board_with_regions(3, 3, [[1, 1, 1], [1, 1, 1], [1, 1, 1]])
        puzzle = Puzzle.from_board(Board(3, 3))
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_valid(b, 1) is True

    def test_invalid_region_no_cells(self) -> None:
        b = Board(3, 3)
        b.cell(0, 0).region_id = 1
        puzzle = Puzzle.from_board(Board(3, 3))
        propagator = ConstraintPropagator(puzzle)
        # Only 1 cell, not empty, should be valid
        b2 = Board(3, 3)
        puzzle2 = Puzzle.from_board(b2)
        propagator2 = ConstraintPropagator(puzzle2)
        b_all = board_with_regions(3, 3, [[1, 1, 1], [1, 1, 1], [1, 1, 1]])
        b_all.cell(0, 0).region_id = 1  # all cells already region 1
        region_1_cells = [c for c in b_all.cells() if c.region_id == 1]
        assert len(region_1_cells) > 0
        assert propagator2.check_region_valid(b_all, 1) is True

    def test_precise_region_under_limit(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [1, 1]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.precise(4)])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_valid(b, 1) is True

    def test_precise_region_over_limit(self) -> None:
        b = board_with_regions(3, 3, [[1, 1, 1], [1, 1, 1], [1, 1, 1]])
        puzzle = Puzzle.from_board(Board(3, 3), rules=[Rule.precise(4)])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_valid(b, 1) is False

    def test_range_region_valid(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [1, 1]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.range(2, 4)])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_valid(b, 1) is True

    def test_range_region_too_small(self) -> None:
        b = board_with_regions(2, 2, [[1, 0], [0, 0]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.range(2, 4)])
        propagator = ConstraintPropagator(puzzle)
        # Region 1 has only 1 cell, below min
        assert propagator.check_region_valid(b, 1) is False

    def test_range_region_too_large(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [1, 1]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.range(1, 2)])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_valid(b, 1) is False

    def test_area_check_with_number_clue(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [1, 1]])
        b.cell(0, 0).number = 4
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.area()])
        propagator = ConstraintPropagator(puzzle)
        # area=4, number clue=4, number < area is False, so valid
        assert propagator.check_region_valid(b, 1) is True

    def test_area_check_fails_when_under_clue(self) -> None:
        b = board_with_regions(3, 3, [[1, 1, 1], [1, 1, 1], [1, 1, 1]])
        b.cell(0, 0).number = 9
        puzzle = Puzzle.from_board(Board(3, 3), rules=[Rule.area()])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_valid(b, 1) is True

    def test_area_check_ok_when_number_bigger_than_area(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        b.cell(0, 0).number = 3
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.area()])
        propagator = ConstraintPropagator(puzzle)
        # check_region_valid only rejects if area exceeds the clue (number < area)
        # area=2, number=3: 3 < 2 is False -> not rejected -> True
        assert propagator.check_region_valid(b, 1) is True

    def test_empty_cell_list(self) -> None:
        b = Board(3, 3)
        puzzle = Puzzle.from_board(Board(3, 3))
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_valid(b, 1) is False


class TestCheckRegionShape:
    def test_no_shape_rules(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        puzzle = Puzzle.from_board(Board(2, 2))
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is True

    def test_shape_pool_match(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        pool = [Shape(cells=frozenset([(0, 0), (0, 1)]))]
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.shape_pool(pool)])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is True

    def test_shape_pool_no_match(self) -> None:
        b = board_with_regions(2, 2, [[1, 0], [0, 0]])
        pool = [Shape(cells=frozenset([(0, 0), (0, 1)]))]
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.shape_pool(pool)])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is False

    def test_puzzle_piece_match(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).shape_pattern = Shape(cells=frozenset([(0, 0), (0, 1)]))
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.puzzle_piece()])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is True

    def test_puzzle_piece_no_match(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).shape_pattern = Shape(cells=frozenset([(0, 0)]))
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.puzzle_piece()])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is False

    def test_block_rule_rectangle_pass(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.block()])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is True

    def test_block_rule_non_rectangle_fail(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [1, 2]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.block()])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is False

    def test_non_block_rule_non_rectangle_pass(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [1, 2]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.non_block()])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is True

    def test_non_block_rule_rectangle_fail(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.non_block()])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is False

    def test_precise_rule_match(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.precise(2)])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is True

    def test_precise_rule_no_match(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.precise(3)])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is False

    def test_solitary_rule_pass(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).symbol = "A"
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.solitary()])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is True

    def test_solitary_rule_fail(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).symbol = "A"
        b.cell(0, 1).symbol = "B"
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.solitary()])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is False

    def test_empty_cells_returns_false(self) -> None:
        b = Board(3, 3)
        puzzle = Puzzle.from_board(Board(3, 3))
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_shape(b, 1) is False


class TestCheckRegionComplete:
    def test_complete_no_rules(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        puzzle = Puzzle.from_board(Board(2, 2))
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_complete(b, 1) is True

    def test_area_rule_satisfied(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).number = 2
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.area()])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_complete(b, 1) is True

    def test_area_rule_not_satisfied(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).number = 1
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.area()])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_complete(b, 1) is False

    def test_range_rule_satisfied(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.range(2, 3)])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_complete(b, 1) is True

    def test_range_rule_not_satisfied(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.range(3, 5)])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_complete(b, 1) is False

    def test_solitary_rule_satisfied(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).symbol = "A"
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.solitary()])
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_complete(b, 1) is True

    def test_solitary_rule_fails_no_symbols(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.solitary()])
        propagator = ConstraintPropagator(puzzle)
        # No symbols in region, len(symbols)=0 != 1 -> fails
        assert propagator.check_region_complete(b, 1) is False

    def test_empty_cells_returns_false(self) -> None:
        b = Board(3, 3)
        puzzle = Puzzle.from_board(Board(3, 3))
        propagator = ConstraintPropagator(puzzle)
        assert propagator.check_region_complete(b, 1) is False

    def test_solitary_region_with_one_symbol_passes(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        b.cell(0, 0).symbol = "A"
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.solitary()])
        propagator = ConstraintPropagator(puzzle)
        # Only 1 symbol in region
        assert propagator.check_region_complete(b, 1) is True


class TestIsRegionComplete:
    def test_region_complete_surrounded(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        puzzle = Puzzle.from_board(Board(2, 2))
        propagator = ConstraintPropagator(puzzle)
        # Region 1 has cells (0,0) and (0,1); neighbors (1,0)=region 2, (1,1)=region 2 - all assigned
        assert propagator.is_region_complete(b, 1) is True

    def test_region_not_complete(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).region_id = 1
        b.cell(1, 0).region_id = 2
        b.cell(1, 1).region_id = 2
        # (0,1) is unassigned (region_id=None)
        puzzle = Puzzle.from_board(Board(2, 2))
        propagator = ConstraintPropagator(puzzle)
        # Region 1: cell (0,0); neighbor (0,1) is unassigned
        assert propagator.is_region_complete(b, 1) is False

    def test_region_edge_not_complete(self) -> None:
        b = Board(3, 3)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        b.cell(1, 0).region_id = 2
        b.cell(1, 1).region_id = 2
        puzzle = Puzzle.from_board(Board(3, 3))
        propagator = ConstraintPropagator(puzzle)
        # Region 1: (0,0),(0,1); neighbor (0,2) is unassigned
        assert propagator.is_region_complete(b, 1) is False


class TestGetAdjacentRegionIds:
    def test_two_adjacent_regions(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        puzzle = Puzzle.from_board(Board(2, 2))
        propagator = ConstraintPropagator(puzzle)
        adj = propagator.get_adjacent_region_ids(b, 1)
        assert adj == {2}

    def test_no_adjacent(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [1, 1]])
        puzzle = Puzzle.from_board(Board(2, 2))
        propagator = ConstraintPropagator(puzzle)
        adj = propagator.get_adjacent_region_ids(b, 1)
        assert adj == set()

    def test_multiple_adjacent(self) -> None:
        b = board_with_regions(2, 2, [[1, 2], [3, 4]])
        puzzle = Puzzle.from_board(Board(2, 2))
        propagator = ConstraintPropagator(puzzle)
        adj = propagator.get_adjacent_region_ids(b, 1)
        assert adj == {2, 3}


class TestFindConnectivityViolations:
    def test_all_connected(self) -> None:
        b = board_with_regions(3, 3, [
            [1, 1, 1],
            [1, 1, 1],
            [1, 1, 1],
        ])
        puzzle = Puzzle.from_board(Board(3, 3))
        propagator = ConstraintPropagator(puzzle)
        assert propagator.find_connectivity_violations(b) == []

    def test_disconnected_region_found(self) -> None:
        b = board_with_regions(3, 3, [
            [1, 0, 1],
            [0, 0, 0],
            [1, 0, 1],
        ])
        puzzle = Puzzle.from_board(Board(3, 3))
        propagator = ConstraintPropagator(puzzle)
        violations = propagator.find_connectivity_violations(b)
        assert 1 in violations

    def test_multiple_disconnected_regions(self) -> None:
        b = board_with_regions(3, 3, [
            [1, 0, 2],
            [0, 0, 0],
            [1, 0, 2],
        ])
        puzzle = Puzzle.from_board(Board(3, 3))
        propagator = ConstraintPropagator(puzzle)
        violations = propagator.find_connectivity_violations(b)
        assert 1 in violations
        assert 2 in violations

    def test_no_assigned_cells(self) -> None:
        b = Board(3, 3)
        puzzle = Puzzle.from_board(Board(3, 3))
        propagator = ConstraintPropagator(puzzle)
        assert propagator.find_connectivity_violations(b) == []

    def test_single_cell_per_region(self) -> None:
        b = board_with_regions(2, 2, [[1, 2], [3, 4]])
        puzzle = Puzzle.from_board(Board(2, 2))
        propagator = ConstraintPropagator(puzzle)
        assert propagator.find_connectivity_violations(b) == []


class TestUpdateBoundaryEdges:
    def test_update_boundary_between_regions(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        update_boundary_edges(b)
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        assert e.is_boundary is True

    def test_update_boundary_inside_region(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [1, 1]])
        update_boundary_edges(b)
        e = b.edge_between(0, 0, 0, 1)
        assert e is not None
        assert e.is_boundary is False

    def test_update_boundary_skips_unassigned(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        # (1,0) and (1,1) unassigned
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        e.is_boundary = True
        update_boundary_edges(b)
        # Should remain unchanged because (1,0) is unassigned
        assert e.is_boundary is True

    def test_update_boundary_clears_when_same_region(self) -> None:
        b = board_with_regions(2, 2, [[1, 1], [2, 2]])
        e = b.edge_between(0, 0, 0, 1)
        assert e is not None
        e.is_boundary = True
        update_boundary_edges(b)
        # Same region, should be False
        assert e.is_boundary is False

    def test_update_boundary_with_multiple_edges(self) -> None:
        b = board_with_regions(2, 2, [[1, 2], [3, 4]])
        update_boundary_edges(b)
        # All edges should be boundaries since all adjacent pairs are different regions
        for e in b.edges():
            assert e.is_boundary is True
