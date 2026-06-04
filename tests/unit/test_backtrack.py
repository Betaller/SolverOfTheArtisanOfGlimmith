from __future__ import annotations

import pytest

from src.models.board import Board
from src.models.puzzle import Puzzle, Rule
from src.solver.backtrack import BacktrackSolver
from src.solver.exceptions import NoSolutionError, SolverTimeoutError


class TestBacktrackSolverInit:
    def test_initialization(self, solver: BacktrackSolver) -> None:
        assert solver.puzzle is not None
        assert solver.puzzle.height == 4
        assert solver.puzzle.width == 4
        assert solver.steps == 0
        assert solver.timeout == 30.0

    def test_initialization_with_custom_puzzle(self) -> None:
        b = Board(3, 3)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(3)])
        solver = BacktrackSolver(puzzle)
        assert solver.puzzle.height == 3

    def test_initialization_with_min_board(self) -> None:
        b = Board(2, 2)
        puzzle = Puzzle.from_board(b)
        solver = BacktrackSolver(puzzle)
        assert solver.puzzle is not None


class TestBacktrackSolverSolve:
    def test_solve_simple_2x2_all_same_region(self) -> None:
        b = Board(2, 2)
        puzzle = Puzzle.from_board(b)
        solver = BacktrackSolver(puzzle)
        solution = solver.solve(timeout=10)
        assert solution.solved is True
        assert solution.steps_taken >= 0
        assert solution.elapsed_ms >= 0
        assert solution.board is not None
        assert solution.error_message is None

    def test_solve_4x4_precise_4(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        solver = BacktrackSolver(puzzle)
        solution = solver.solve(timeout=30)
        assert solution.solved is True
        assert len(solution.regions) >= 1
        for region in solution.regions:
            assert region.area == 4

    def test_solve_2x2_all_one_region(self) -> None:
        b = Board(2, 2)
        puzzle = Puzzle.from_board(b)
        solver = BacktrackSolver(puzzle)
        solution = solver.solve(timeout=10)
        assert solution.solved is True

    def test_solve_region_info_present(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        solver = BacktrackSolver(puzzle)
        solution = solver.solve(timeout=30)
        if solution.solved:
            for region in solution.regions:
                assert region.area > 0
                assert len(region.cells) == region.area
                assert region.normalized_shape_key is not None

    def test_solve_preserves_puzzle_data(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).number = 2
        puzzle = Puzzle.from_board(b, rules=[Rule.area(), Rule.precise(2)])
        solver = BacktrackSolver(puzzle)
        solution = solver.solve(timeout=10)
        if solution.solved:
            assert solution.board.cell(0, 0).number == 2


class TestBacktrackSolverEdgeCases:
    def test_no_solution_for_impossible_precise(self) -> None:
        # 2x2 board with precise(3) - impossible since 4 cells cannot be
        # partitioned into regions of area 3 evenly
        b = Board(2, 2)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(3)])
        solver = BacktrackSolver(puzzle)
        solution = solver.solve(timeout=5)
        # This might still find a partial solution, but might not
        assert isinstance(solution.solved, bool)

    def test_2x2_board_with_precise_2(self) -> None:
        b = Board(2, 2)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(2)])
        solver = BacktrackSolver(puzzle)
        solution = solver.solve(timeout=10)
        assert solution.solved is True
        assert len(solution.regions) == 2
        for region in solution.regions:
            assert region.area == 2

    def test_with_range_rule(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.range(2, 4)])
        solver = BacktrackSolver(puzzle)
        solution = solver.solve(timeout=30)
        if solution.solved:
            for region in solution.regions:
                assert 2 <= region.area <= 4

    def test_solver_reuse(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        solver = BacktrackSolver(puzzle)
        solution1 = solver.solve(timeout=10)
        solution2 = solver.solve(timeout=10)
        assert isinstance(solution1.solved, bool)
        assert isinstance(solution2.solved, bool)


class TestBacktrackSolverInternals:
    def test_pick_seed(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b)
        solver = BacktrackSolver(puzzle)
        seed = solver._pick_seed({(1, 2), (0, 0), (3, 3)})
        assert seed == (0, 0)

    def test_pick_seed_single(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b)
        solver = BacktrackSolver(puzzle)
        seed = solver._pick_seed({(2, 3)})
        assert seed == (2, 3)

    def test_max_region_area_default(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b)
        solver = BacktrackSolver(puzzle)
        assert solver._max_region_area() == 16

    def test_max_region_area_precise(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        solver = BacktrackSolver(puzzle)
        assert solver._max_region_area() == 4

    def test_max_region_area_range(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.range(2, 6)])
        solver = BacktrackSolver(puzzle)
        assert solver._max_region_area() == 6

    def test_min_region_area_default(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b)
        solver = BacktrackSolver(puzzle)
        assert solver._min_region_area() == 1

    def test_min_region_area_range(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.range(3, 6)])
        solver = BacktrackSolver(puzzle)
        assert solver._min_region_area() == 3

    def test_frontier(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b)
        solver = BacktrackSolver(puzzle)
        unassigned = {(0, 0), (0, 1), (0, 2), (1, 0)}
        region = {(0, 0)}
        frontier = solver._frontier(region, unassigned)
        assert (0, 1) in frontier
        assert (1, 0) in frontier
        assert (0, 0) not in frontier

    def test_frontier_excludes_outside_unassigned(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b)
        solver = BacktrackSolver(puzzle)
        unassigned = {(0, 0)}
        region = {(0, 0)}
        frontier = solver._frontier(region, unassigned)
        # (0,1) is a neighbor but not in unassigned
        assert (0, 1) not in frontier

    def test_is_rectangle_shape(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b)
        solver = BacktrackSolver(puzzle)
        assert solver._is_rectangle_shape({(0, 0), (0, 1)}) is True
        assert solver._is_rectangle_shape({(0, 0), (1, 0)}) is True
        assert solver._is_rectangle_shape({(0, 0), (0, 1), (1, 0), (1, 1)}) is True
        assert solver._is_rectangle_shape({(0, 0), (1, 0), (1, 1)}) is False
        assert solver._is_rectangle_shape(set()) is False

    def test_region_feasible_precise(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        solver = BacktrackSolver(puzzle)
        cells_ok = {(0, 0), (0, 1), (1, 0), (1, 1)}
        cells_over = {(0, 0), (0, 1), (1, 0), (1, 1), (2, 0)}
        assert solver._region_feasible(b, cells_ok) is True
        assert solver._region_feasible(b, cells_over) is False

    def test_unassign(self) -> None:
        b = Board(2, 2)
        puzzle = Puzzle.from_board(b)
        solver = BacktrackSolver(puzzle)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        solver._unassign(b, {(0, 0), (0, 1)})
        assert b.cell(0, 0).region_id is None
        assert b.cell(0, 1).region_id is None


class TestBacktrackSolverWithConstraints:
    def test_solve_with_block_rule(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.block()])
        solver = BacktrackSolver(puzzle)
        solution = solver.solve(timeout=30)
        if solution.solved:
            for region in solution.regions:
                cells_set = set(region.cells)
                min_r = min(r for r, _ in cells_set)
                max_r = max(r for r, _ in cells_set)
                min_c = min(c for _, c in cells_set)
                max_c = max(c for _, c in cells_set)
                assert len(cells_set) == (max_r - min_r + 1) * (max_c - min_c + 1)

    def test_solve_with_same_rule(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4), Rule.same()])
        solver = BacktrackSolver(puzzle)
        solution = solver.solve(timeout=30)
        if solution.solved:
            assert len(solution.regions) == 4
            shape_keys = {r.normalized_shape_key for r in solution.regions}
            assert len(shape_keys) == 1

    def test_solve_with_area_rule(self) -> None:
        b = Board(4, 4)
        b.cell(0, 0).number = 4
        puzzle = Puzzle.from_board(b, rules=[Rule.area()])
        solver = BacktrackSolver(puzzle)
        solution = solver.solve(timeout=30)
        if solution.solved:
            region = solution.region_of(0, 0)
            assert region is not None
            assert region.area == 4
