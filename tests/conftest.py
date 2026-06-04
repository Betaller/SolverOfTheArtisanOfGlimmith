from __future__ import annotations

import pytest

from src.models.board import Board, Cell, Shape, CompassClue
from src.models.puzzle import Puzzle, Rule
from src.solver.backtrack import BacktrackSolver
from src.solver.validator import SolutionValidator
from src.solver.shapes import enumerate_polyominoes


@pytest.fixture
def empty_board_4x4() -> Board:
    return Board(4, 4)


@pytest.fixture
def empty_board_6x6() -> Board:
    return Board(6, 6)


@pytest.fixture
def puzzle_no_rules_4x4() -> Puzzle:
    board = Board(4, 4)
    return Puzzle.from_board(board)


@pytest.fixture
def puzzle_with_area_clues() -> Puzzle:
    board = Board(4, 4)
    board.cell(0, 0).number = 4
    board.cell(2, 2).number = 4
    return Puzzle.from_board(board, rules=[Rule(type="area")])


@pytest.fixture
def puzzle_precise_4() -> Puzzle:
    board = Board(4, 4)
    return Puzzle.from_board(board, rules=[Rule(type="precise", params={"area": 4})])


@pytest.fixture
def sample_shapes() -> dict[str, Shape]:
    shapes = enumerate_polyominoes(4)
    return {_shape_name(s): s for s in shapes}


def _shape_name(s: Shape) -> str:
    cells = sorted(s.cells)
    if len(cells) == 1:
        return "monomino"
    if len(cells) == 2:
        return "domino"
    if len(cells) == 3:
        if cells == [(0, 0), (0, 1), (0, 2)]:
            return "I3"
        return "L3"
    if len(cells) == 4:
        if cells == [(0, 0), (0, 1), (0, 2), (0, 3)]:
            return "I4"
        if cells == [(0, 0), (0, 1), (1, 0), (1, 1)]:
            return "O"
        if cells == [(0, 0), (0, 1), (0, 2), (1, 0)]:
            return "L4"
        if cells == [(0, 0), (0, 1), (0, 2), (1, 1)]:
            return "T4"
        if cells == [(0, 0), (0, 1), (1, 1), (1, 2)]:
            return "S4"
    return f"shape_{len(cells)}"


@pytest.fixture
def solver() -> BacktrackSolver:
    puzzle = Puzzle.from_board(Board(4, 4), rules=[Rule(type="precise", params={"area": 4})])
    return BacktrackSolver(puzzle)


@pytest.fixture
def validator() -> SolutionValidator:
    return SolutionValidator()


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
