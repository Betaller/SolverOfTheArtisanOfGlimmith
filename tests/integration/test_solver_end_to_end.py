from __future__ import annotations

from src.models.board import Board, Shape
from src.models.puzzle import Puzzle, Rule
from src.solver.base import default_router
from src.solver.shapes import canonical_key
from src.validation.validator import IndependentValidator, solution_to_board


def make_puzzle_with_boundaries(
    height: int, width: int,
    boundaries: list[tuple[int, int, int, int]],
    blocked_cells: list[tuple[int, int]] | None = None,
) -> Puzzle:
    board = Board(height, width)
    for (r1, c1, r2, c2) in boundaries:
        e = board.edge_between(r1, c1, r2, c2)
        if e is not None:
            e.is_boundary = True
        else:
            e = board.edge_between(r2, c2, r1, c1)
            if e is not None:
                e.is_boundary = True
    if blocked_cells:
        for r, c in blocked_cells:
            board.cell(r, c).blocked = True
    return Puzzle.from_board(board)


def apply_solution_to_board(board: Board, region_map: list[list[int]]) -> Board:
    for r in range(board.height):
        for c in range(board.width):
            board.cell(r, c).region_id = region_map[r][c]
    for e in board.edges():
        c1 = board.cell(e.r1, e.c1)
        c2 = board.cell(e.r2, e.c2)
        if c1.assigned and c2.assigned:
            e.is_boundary = c1.region_id != c2.region_id
    return board


def solve(puzzle: Puzzle, timeout: float = 30.0):
    """Run the default (Rust-only) router, which re-verifies every answer."""
    return default_router().route(puzzle, timeout=timeout)


def validate(puzzle: Puzzle, board: Board):
    return IndependentValidator().validate(puzzle, board)


class TestEndToEndNoRules:
    def test_solve_2x2_board(self) -> None:
        b = Board(2, 2)
        puzzle = Puzzle.from_board(b)
        solution = solve(puzzle, timeout=10)
        assert solution.solved is True
        for region in solution.regions:
            assert len(region.cells) == region.area

    def test_solve_2x2_precise_1(self) -> None:
        b = Board(2, 2)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(1)])
        solution = solve(puzzle, timeout=10)
        assert solution.solved is True
        assert len(solution.regions) == 4

    def test_solve_4x4_precise_4(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        solution = solve(puzzle, timeout=10)
        assert solution.solved is True
        assert len(solution.regions) == 4

    def test_validator_accepts_solver_output(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        solution = solve(puzzle, timeout=10)
        assert solution.solved is True
        validation = validate(puzzle, solution_to_board(puzzle, solution))
        assert validation.solved is True


class TestEndToEndPreciseRule:
    def test_solve_4x4_precise_4(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        solution = solve(puzzle, timeout=30)
        assert solution.solved is True
        assert len(solution.regions) == 4
        for region in solution.regions:
            assert region.area == 4

    def test_solve_4x4_precise_2(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(2)])
        solution = solve(puzzle, timeout=30)
        assert solution.solved is True
        assert len(solution.regions) == 8
        for region in solution.regions:
            assert region.area == 2

    def test_solve_3x3_precise_3(self) -> None:
        b = Board(3, 3)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(3)])
        solution = solve(puzzle, timeout=30)
        assert solution.solved is True
        assert len(solution.regions) == 3
        for region in solution.regions:
            assert region.area == 3

    def test_validator_accepts_precise_solution(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        solution = solve(puzzle, timeout=30)
        assert solution.solved is True
        validation = validate(puzzle, solution_to_board(puzzle, solution))
        assert validation.solved is True
        assert validation.rule_results.get("precise") is True


class TestEndToEndRangeRule:
    def test_solve_4x4_range_2_4(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.range(2, 4)])
        solution = solve(puzzle, timeout=30)
        assert solution.solved is True
        for region in solution.regions:
            assert 2 <= region.area <= 4

    def test_solve_4x4_range_4_4_equivalent_to_precise(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.range(4, 4)])
        solution = solve(puzzle, timeout=30)
        assert solution.solved is True
        for region in solution.regions:
            assert region.area == 4


class TestEndToEndAreaRule:
    def test_solve_with_area_clues(self, puzzle_with_area_clues: Puzzle) -> None:
        solution = solve(puzzle_with_area_clues, timeout=30)
        assert solution.solved is True
        region0 = solution.region_of(0, 0)
        region2 = solution.region_of(2, 2)
        assert region0 is not None
        assert region2 is not None
        assert region0.area == 4
        assert region2.area == 4


class TestEndToEndBlockRule:
    def test_block_rule_validates_existing_solution(self) -> None:
        b = Board(2, 2)
        for r in range(2):
            for c in range(2):
                b.cell(r, c).region_id = 0
        apply_solution_to_board(b, [[0, 0], [0, 0]])
        puzzle = Puzzle.from_board(Board(2, 2), rules=[Rule.block()])
        validation = validate(puzzle, b)
        assert validation.solved is True


class TestEndToEndShapePool:
    def test_solve_with_shape_pool_dominoes_2x4(self) -> None:
        b = Board(2, 4)
        domino = Shape(cells=frozenset([(0, 0), (0, 1)]))
        puzzle = Puzzle.from_board(b, rules=[Rule.shape_pool([domino])])
        solution = solve(puzzle, timeout=10)
        assert solution.solved is True
        for region in solution.regions:
            assert region.area == 2


class TestEndToEndSameRule:
    def test_solve_4x4_precise_4_same(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4), Rule.same()])
        solution = solve(puzzle, timeout=30)
        assert solution.solved is True
        # Rust reports the placed (non-normalized) shape key; compute the
        # dihedral canonical key from the cells to check 'same' semantics.
        shape_keys = {canonical_key(frozenset(r.cells)) for r in solution.regions}
        assert len(shape_keys) == 1


class TestEndToEndMixedConstraints:
    def test_solve_2x4_precise_2_different(self) -> None:
        b = Board(2, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(2), Rule.different()])
        solution = solve(puzzle, timeout=10)
        # Dominoes all share the same canonical key, so 'different' cannot be satisfied
        assert solution.solved is False


class TestEndToEndSolitaryRule:
    def test_solve_2x2_solitary_with_precise(self) -> None:
        b = Board(2, 2)
        b.cell(0, 0).symbol = "A"
        b.cell(1, 0).symbol = "B"
        puzzle = Puzzle.from_board(b, rules=[Rule.solitary(), Rule.precise(2)])
        solution = solve(puzzle, timeout=10)
        assert solution.solved is True
        assert len(solution.regions) == 2
        region_a = solution.region_of(0, 0)
        region_b = solution.region_of(1, 0)
        assert region_a is not None
        assert region_b is not None


class TestEndToEndPreciseRangeConflict:
    def test_conflicting_rules_still_solves(self) -> None:
        # precise and range together - game logic allows both but range is redundant
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4), Rule.range(2, 6)])
        solution = solve(puzzle, timeout=30)
        assert solution.solved is True
        for region in solution.regions:
            assert region.area == 4


class TestEndToEndValidationCycle:
    def test_full_solve_and_validate_cycle(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        solution = solve(puzzle, timeout=30)
        assert solution.solved is True

        # Re-validate independently
        validation = validate(puzzle, solution_to_board(puzzle, solution))
        assert validation.solved is True

        # Check solution metadata (Rust aog path does not report step counts)
        assert solution.elapsed_ms >= 0


class TestEndToEndSmallBoards:
    def test_solve_all_2x2_precise_1(self) -> None:
        b = Board(2, 2)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(1)])
        solution = solve(puzzle, timeout=10)
        assert solution.solved is True
        assert len(solution.regions) == 4
        for region in solution.regions:
            assert region.area == 1

    def test_solve_2x4_precise_2(self) -> None:
        b = Board(2, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(2)])
        solution = solve(puzzle, timeout=10)
        assert solution.solved is True
        assert len(solution.regions) == 4
        for region in solution.regions:
            assert region.area == 2

    def test_solve_3x4_precise_3(self) -> None:
        b = Board(3, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(3)])
        solution = solve(puzzle, timeout=30)
        assert solution.solved is True
        assert len(solution.regions) == 4
        for region in solution.regions:
            assert region.area == 3


class TestEndToEndValidatorWithKnownSolution:
    def test_known_valid_4x4_solution(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b, rules=[Rule.precise(4)])
        region_map = [
            [1, 1, 2, 2],
            [1, 1, 2, 2],
            [3, 3, 4, 4],
            [3, 3, 4, 4],
        ]
        apply_solution_to_board(b, region_map)
        solution = validate(puzzle, b)
        assert solution.solved is True
        assert len(solution.errors) == 0

    def test_known_invalid_disconnected_solution(self) -> None:
        b = Board(4, 4)
        puzzle = Puzzle.from_board(b)
        region_map = [
            [1, 1, 1, 1],
            [0, 0, 0, 0],
            [1, 1, 1, 1],
            [0, 0, 0, 0],
        ]
        apply_solution_to_board(b, region_map)
        solution = validate(puzzle, b)
        # Even though the board is complete, a region is disconnected
        assert solution.solved is False
        assert any("连通" in e for e in solution.errors)


class TestEndToEndPreDrawnBoundaries:
    def test_solve_with_boundaries_no_rules(self) -> None:
        puzzle = make_puzzle_with_boundaries(3, 3, [
            (0, 1, 0, 2), (1, 1, 1, 2),
        ])
        solution = solve(puzzle, timeout=10)
        assert solution.solved is True
        board = solution_to_board(puzzle, solution)
        for (r1, c1, r2, c2) in [(0, 1, 0, 2), (1, 1, 1, 2)]:
            e = board.edge_between(r1, c1, r2, c2)
            assert e is not None
            c1_cell = board.cell(r1, c1)
            c2_cell = board.cell(r2, c2)
            assert c1_cell.region_id != c2_cell.region_id

    def test_solve_6x5_bordered_with_boundaries(self) -> None:
        blocked = [
            (0, 0), (0, 1), (0, 2), (0, 3), (0, 4),
            (1, 0), (1, 4),
            (2, 0), (2, 4),
            (3, 0), (3, 4),
            (4, 0), (4, 4),
            (5, 0), (5, 1), (5, 2), (5, 3), (5, 4),
        ]
        boundaries = [
            (0, 1, 1, 1), (0, 2, 1, 2), (0, 3, 1, 3),
            (1, 3, 1, 4), (2, 0, 2, 1), (2, 2, 2, 3),
            (2, 3, 2, 4), (3, 0, 3, 1), (3, 1, 3, 2),
            (3, 3, 3, 4), (4, 0, 4, 1), (4, 3, 4, 4),
            (1, 1, 2, 1), (1, 2, 2, 2), (1, 3, 2, 3),
            (2, 2, 3, 2), (3, 2, 4, 2),
            (4, 1, 5, 1), (4, 2, 5, 2), (4, 3, 5, 3),
        ]
        puzzle = make_puzzle_with_boundaries(6, 5, boundaries, blocked)
        solution = solve(puzzle, timeout=30)
        assert solution.solved is True
        board = solution_to_board(puzzle, solution)
        violations = 0
        for r1, c1, r2, c2 in boundaries:
            c1_cell = board.cell(r1, c1)
            c2_cell = board.cell(r2, c2)
            if c1_cell.assigned and c2_cell.assigned and c1_cell.region_id == c2_cell.region_id:
                violations += 1
        assert violations == 0, f"{violations} pre-drawn boundaries violated"

    def test_boundary_enforced_during_search(self) -> None:
        puzzle = make_puzzle_with_boundaries(2, 3, [(0, 1, 0, 2)])
        solution = solve(puzzle, timeout=10)
        assert solution.solved is True
        board = solution_to_board(puzzle, solution)
        c1 = board.cell(0, 1)
        c2 = board.cell(0, 2)
        assert c1.assigned and c2.assigned
        assert c1.region_id != c2.region_id
