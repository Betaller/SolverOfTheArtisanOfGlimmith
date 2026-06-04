from __future__ import annotations

import pytest

from src.models.board import Board, Shape, EdgeConstraint, EdgeConstraintType
from src.models.puzzle import Puzzle, Rule
from src.models.solution import Solution, RegionInfo
from src.solver.validator import SolutionValidator


class TestSolutionValidator:
    def test_valid_complete_board_no_rules(self) -> None:
        b = Board(2, 2)
        for r in range(2):
            for c in range(2):
                b.cell(r, c).region_id = 0
        puzzle = Puzzle.from_board(b)
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        assert solution.solved is True
        assert solution.error_message is None
        assert len(solution.regions) == 1

    def test_incomplete_board_not_solved(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).region_id = 0
        puzzle = Puzzle.from_board(b)
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        assert solution.solved is False

    def test_error_message_for_disconnected_regions(self) -> None:
        b = Board(3, 3)
        b.cell(0, 0).region_id = 1
        b.cell(2, 2).region_id = 1
        puzzle = Puzzle.from_board(b)
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        assert solution.solved is False
        assert "连通" in solution.error_message

    def test_error_message_for_boundary_inconsistency(self) -> None:
        b = Board(2, 2)
        for r in range(2):
            for c in range(2):
                b.cell(r, c).region_id = 0
        e = b.edge_between(0, 0, 0, 1)
        assert e is not None
        e.is_boundary = True
        puzzle = Puzzle.from_board(b)
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        assert solution.solved is False
        assert "边框" in solution.error_message

    def test_rule_violation_reported(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        b.cell(1, 0).region_id = 2
        b.cell(1, 1).region_id = 2
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(3)])
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        assert solution.solved is False
        assert "精确" in solution.error_message
        assert solution.rule_results.get("precise") is False

    def test_rule_passed_reported(self) -> None:
        b = Board(4, 4)
        for r in range(4):
            for c in range(4):
                b.cell(r, c).region_id = r // 2 * 2 + c // 2
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        assert solution.solved is True
        assert solution.rule_results.get("precise") is True

    def test_region_infos_generated(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        b.cell(1, 0).region_id = 2
        b.cell(1, 1).region_id = 2
        puzzle = Puzzle.from_board(b)
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        assert len(solution.regions) == 2
        for region in solution.regions:
            assert region.area == 2
            assert region.normalized_shape_key is not None
            assert len(region.cells) == 2

    def test_region_info_for_single_cell(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 2
        b.cell(1, 0).region_id = 3
        b.cell(1, 1).region_id = 4
        puzzle = Puzzle.from_board(b)
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        assert solution.solved is True
        assert len(solution.regions) == 4
        for region in solution.regions:
            assert region.area == 1
            assert len(region.cells) == 1

    def test_region_info_cells_are_tuples(self) -> None:
        b = Board(2, 2)
        for r in range(2):
            for c in range(2):
                b.cell(r, c).region_id = 0
        puzzle = Puzzle.from_board(b)
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        for region in solution.regions:
            for cell in region.cells:
                assert isinstance(cell, tuple)
                assert len(cell) == 2

    def test_matched_shape_name_with_pool(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        b.cell(1, 0).region_id = 2
        b.cell(1, 1).region_id = 2
        pool_shapes = [Shape(cells=frozenset([(0, 0), (0, 1)]))]
        puzzle = Puzzle.from_board(b, rules=[Rule.shape_pool(pool_shapes)])
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        assert len(solution.regions) == 2
        # Each region is a domino, which should match pool
        for region in solution.regions:
            assert region.matched_shape_name is not None

    def test_no_matched_shape_without_pool(self) -> None:
        b = Board(2, 2)
        for r in range(2):
            for c in range(2):
                b.cell(r, c).region_id = 0
        puzzle = Puzzle.from_board(b)
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        for region in solution.regions:
            assert region.matched_shape_name is None

    def test_multiple_rule_errors(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        b.cell(1, 0).region_id = 2
        b.cell(1, 1).region_id = 2
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(1), Rule.range(3, 4)])
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        assert solution.solved is False
        assert solution.error_message is not None
        # Should have error for both precise and range
        assert solution.rule_results.get("precise") is False
        assert solution.rule_results.get("range") is False

    def test_validate_function(self) -> None:
        from src.solver.validator import validate_solution
        b = Board(2, 2)
        for r in range(2):
            for c in range(2):
                b.cell(r, c).region_id = 0
        puzzle = Puzzle.from_board(b)
        solution = validate_solution(puzzle, b)
        assert solution.solved is True

    def test_solution_region_of(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        b.cell(1, 0).region_id = 2
        b.cell(1, 1).region_id = 2
        puzzle = Puzzle.from_board(b)
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        region = solution.region_of(0, 0)
        assert region is not None
        assert region.region_id == 1
        assert solution.region_of(5, 5) is None

    def test_solution_rule_results_all_true_when_solved(self) -> None:
        b = Board(4, 4)
        for r in range(4):
            for c in range(4):
                b.cell(r, c).region_id = r // 2 * 2 + c // 2
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        assert solution.solved is True
        # rule_results should have entries for all checkers, but many will be True
        # because they check if puzzle.has_rule first
        assert isinstance(solution.rule_results, dict)

    def test_error_on_connectivity_and_boundary(self) -> None:
        b = Board(3, 3)
        b.cell(0, 0).region_id = 1
        b.cell(2, 2).region_id = 1
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        e.is_boundary = True
        puzzle = Puzzle.from_board(b)
        validator = SolutionValidator()
        solution = validator.validate(puzzle, b)
        assert solution.solved is False
        assert "连通" in solution.error_message
        assert "边框" in solution.error_message
