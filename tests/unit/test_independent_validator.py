from __future__ import annotations

from src.models.board import Board, CompassClue, Shape
from src.models.puzzle import Puzzle, Rule
from src.models.solution import RegionInfo, Solution
from src.validation.validator import IndependentValidator, _canonical_key, solution_to_board


def _puzzle(rules, h=4, w=4) -> Puzzle:
    b = Board(h, w)
    return Puzzle(
        height=h, width=w, cells=b.cells(), edges=b.edges(), vertices=b.vertices(), rules=rules
    )


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
        puzzle = _puzzle(
            [Rule("shape_pool", {"shapes": [_shape(((0, 0), (0, 1), (1, 0), (1, 1)))]})], h=2, w=2
        )
        board = _board(2, 2, [[(0, 0), (0, 1), (1, 0), (1, 1)]])
        res = IndependentValidator().validate(puzzle, board)
        assert res.solved


class TestIndependentValidatorCompass:
    def test_halfplane_rule(self) -> None:
        puzzle = _puzzle([Rule.compass()], h=3, w=3)
        board = _board(
            3,
            3,
            [
                [(0, 0), (0, 1), (1, 0), (1, 1)],
                [(0, 2), (1, 2)],
                [(2, 0), (2, 1), (2, 2)],
            ],
        )
        # region 0 cells to the right of (0,0): (0,1),(1,1) -> 2
        board.cell(0, 0).compass = CompassClue(up=-1, down=-1, left=-1, right=2)
        res = IndependentValidator().validate(puzzle, board)
        assert res.solved

    def test_halfplane_wrong_value_rejected(self) -> None:
        puzzle = _puzzle([Rule.compass()], h=3, w=3)
        board = _board(
            3,
            3,
            [
                [(0, 0), (0, 1), (1, 0), (1, 1)],
                [(0, 2), (1, 2)],
                [(2, 0), (2, 1), (2, 2)],
            ],
        )
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
        assert _canonical_key(frozenset({(0, 0), (0, 1), (1, 0)})) == _canonical_key(
            frozenset({(0, 0), (0, 1), (1, 1)})
        )
        assert _canonical_key(frozenset({(0, 0), (0, 1), (1, 0)})) != _canonical_key(
            frozenset({(0, 0), (0, 1), (1, 2)})
        )


class TestSolutionToBoardCarriesVertexClues:
    """`solution_to_board` must copy vertex clues, or `_check_watchtower` runs on
    a board whose vertices all carry `watchtower=None` and passes vacuously.

    That hole sat exactly on the router re-validation path (`base.py`) and the
    benchmark — the last gate that is supposed to stop a buggy solver's wrong
    answer (aog's `build_solution_trusted` skips the Rust-side re-validate).
    The hand-built-board unit tests above never caught it because they share
    Vertex objects between puzzle and board; only the rebuild path loses them.
    """

    @staticmethod
    def _solution(regions):
        return Solution(
            regions=[
                RegionInfo(
                    region_id=i,
                    cells=[tuple(c) for c in cells],
                    area=len(cells),
                    shape=Shape(cells=frozenset(cells)),
                    normalized_shape_key=_canonical_key(frozenset(cells)),
                )
                for i, cells in enumerate(regions)
            ]
        )

    def test_watchtower_check_not_vacuous_on_rebuild_path(self) -> None:
        puzzle = _puzzle([Rule("watchtower")], h=2, w=2)
        # Vertex (1,1) touches all 4 cells; value 2 demands exactly 2 regions.
        for v in puzzle.vertices:
            if (v.row, v.col) == (1, 1):
                v.watchtower = 2
        wrong = self._solution([[(0, 0)], [(0, 1)], [(1, 0)], [(1, 1)]])
        board = solution_to_board(puzzle, wrong)
        res = IndependentValidator().validate(puzzle, board)
        assert not res.solved
        assert any("望塔" in e for e in res.errors)

    def test_correct_watchtower_count_passes_on_rebuild_path(self) -> None:
        puzzle = _puzzle([Rule("watchtower")], h=2, w=2)
        for v in puzzle.vertices:
            if (v.row, v.col) == (1, 1):
                v.watchtower = 2
        ok = self._solution([[(0, 0), (0, 1)], [(1, 0), (1, 1)]])
        board = solution_to_board(puzzle, ok)
        res = IndependentValidator().validate(puzzle, board)
        assert res.solved, res.errors

    def test_0994_multi_split_rejected_by_watchtower(self) -> None:
        """Real-world regression: splitting official 0994's 39-cell region at
        its articulation cell (0,0) into {(0,0)} + two components used to pass
        the router re-validation (watchtower vacuous) even though vertex
        (5,4)=2 then sees 3 distinct regions.
        """
        import json
        from pathlib import Path

        base = Path(__file__).resolve().parents[2]
        puz_path = base / "puzzles/official/Zone3/3-vertex-radar/0994.json"
        ans_path = base / "puzzles/official/Zone3-answer/3-vertex-radar/0994.json"
        if not puz_path.exists() or not ans_path.exists():
            import pytest

            pytest.skip("official corpus not present")
        from src.io.puzzle_codec import dict_to_puzzle

        puzzle = dict_to_puzzle(json.loads(puz_path.read_text()))
        official = [list(map(tuple, r)) for r in json.loads(ans_path.read_text())["regions"]]
        rest = [c for c in official[0] if c != (0, 0)]
        # 4-connected components of the 38 remainder cells.
        seen: set[tuple[int, int]] = set()
        comps: list[list[tuple[int, int]]] = []
        for s in sorted(rest):
            if s in seen:
                continue
            stack, comp = [s], []
            seen.add(s)
            while stack:
                r, c = stack.pop()
                comp.append((r, c))
                for dr, dc in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    n = (r + dr, c + dc)
                    if n in rest and n not in seen:
                        seen.add(n)
                        stack.append(n)
            comps.append(comp)
        assert len(comps) == 2  # (0,0) is an articulation cell of the region
        split = [[(0, 0)]] + [list(r) for r in official[1:]] + comps

        good = self._solution(official)
        board = solution_to_board(puzzle, good)
        assert IndependentValidator().validate(puzzle, board).solved

        bad = self._solution(split)
        board = solution_to_board(puzzle, bad)
        res = IndependentValidator().validate(puzzle, board)
        assert not res.solved
        assert any("望塔" in e for e in res.errors)
