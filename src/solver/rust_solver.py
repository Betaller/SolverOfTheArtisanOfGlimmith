from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

from src.io.puzzle_codec import puzzle_to_dict
from src.models.board import Board, Shape
from src.models.puzzle import Puzzle
from src.models.solution import RegionInfo, Solution
from src.solver.base import Solver


def _board_for(puzzle: Puzzle) -> Board:
    """Board carrying the puzzle's blocked cells and pre-drawn boundaries."""
    board = Board(puzzle.height, puzzle.width)
    for c in puzzle.cells:
        dst = board.cell(c.row, c.col)
        dst.number = c.number
        dst.symbol = c.symbol
        dst.shape_pattern = c.shape_pattern
        dst.compass = c.compass
        dst.fence_pattern = c.fence_pattern
        dst.blocked = c.blocked
    for e in puzzle.edges:
        edge = board.edge_between(e.r1, e.c1, e.r2, e.c2)
        if edge is not None:
            edge.is_boundary = e.is_boundary
    return board


def _crosses_boundary(board: Board, r0: int, c0: int, rh: int, rw: int) -> bool:
    """True if a rectangle placement overlaps any pre-drawn boundary edge."""
    for r in range(rh):
        for c in range(rw - 1):
            edge = board.edge_between(r0 + r, c0 + c, r0 + r, c0 + c + 1)
            if edge is not None and edge.is_boundary:
                return True
    for r in range(rh - 1):
        for c in range(rw):
            edge = board.edge_between(r0 + r, c0 + c, r0 + r + 1, c0 + c)
            if edge is not None and edge.is_boundary:
                return True
    return False


def _fitting_rectangles(puzzle: Puzzle) -> list[list[list[int]]]:
    """All rectangle shapes (1x1 .. HxW) that fit at least one valid placement
    on the actual board — within fillable cells and not crossing any pre-drawn
    boundary.  A shape that fits nowhere can never appear in a solution, so it
    is dropped from the synthesized pool, shrinking it on irregular boards
    (many blocked cells / drawn boundaries).

    Used to route `block` puzzles through the shape_pool DLX solver (pieces):
    a region being a solid rectangle is exactly "region shape ∈ pool" when the
    pool is every rectangle that fits the board.
    """
    board = _board_for(puzzle)
    h, w = puzzle.height, puzzle.width
    fillable = {(c.row, c.col) for c in puzzle.cells if not c.blocked}
    shapes: list[list[list[int]]] = []
    for rh in range(1, h + 1):
        for rw in range(1, w + 1):
            fits = False
            for r0 in range(h - rh + 1):
                for c0 in range(w - rw + 1):
                    if any((r0 + r, c0 + c) not in fillable
                           for r in range(rh) for c in range(rw)):
                        continue
                    if _crosses_boundary(board, r0, c0, rh, rw):
                        continue
                    fits = True
                    break
                if fits:
                    break
            if fits:
                shapes.append([[r, c] for r in range(rh) for c in range(rw)])
    return shapes


def _find_binary() -> str:
    candidates = []

    # release build takes priority
    project_root = Path(__file__).parent.parent.parent
    for profile in ["release", "debug"]:
        candidates.append(str(project_root / "rsolver" / "target" / profile / "rsolver.exe"))
        candidates.append(str(project_root / "rsolver" / "target" / profile / "rsolver"))

    for path in candidates:
        if os.path.isfile(path):
            return path

    raise FileNotFoundError(
        "Rust solver binary not found. Build it with: cd rsolver && cargo build --release"
    )


class RustSolver(Solver):
    name = "rust"

    def __init__(self) -> None:
        self._binary = _find_binary()

    # The Rust binary runs three solver parts sequentially (aog → pieces →
    # backtrack), each of which gets the full unit `timeout` as its own
    # deadline.  The subprocess therefore needs 3× wall-clock for every part to
    # use its budget.
    RUST_PARTS = 3

    def solve(self, puzzle: Puzzle, timeout: float = 30.0) -> Solution:
        data = puzzle_to_dict(puzzle)
        # block → shape_pool: hand the Rust pieces (DLX) solver a pool of every
        # rectangle up to the board size, so block puzzles use the exact-cover
        # path.  The final answer is still validated against the original `block`
        # rule by the router, so a non-rectangle tiling can never slip through.
        if puzzle.has_rule("block") and not data.get("shape_pool"):
            data["shape_pool"] = _fitting_rectangles(puzzle)
        input_json = json.dumps(data, ensure_ascii=True)

        try:
            proc = subprocess.run(
                [self._binary],
                input=input_json,
                capture_output=True,
                text=True,
                timeout=timeout * self.RUST_PARTS,
                encoding="utf-8",
            )
        except subprocess.TimeoutExpired:
            return Solution(
                solved=False,
                error_message=f"Rust solver timed out after {timeout:.0f}s",
            )
        except FileNotFoundError:
            return Solution(
                solved=False,
                error_message=f"Rust solver binary not found: {self._binary}",
            )
        except Exception as e:
            return Solution(solved=False, error_message=str(e))

        if proc.returncode != 0:
            err = f"Rust solver exited with code {proc.returncode}: {proc.stderr[:500]}"
            return Solution(solved=False, error_message=err)

        try:
            data = json.loads(proc.stdout)
        except json.JSONDecodeError as e:
            return Solution(solved=False, error_message=f"Invalid JSON from solver: {e}")

        if not data.get("solved"):
            return Solution(
                solved=False,
                steps_taken=data.get("steps_taken", 0),
                elapsed_ms=data.get("elapsed_ms", 0),
                error_message=data.get("error_message", "No solution"),
            )

        regions: list[RegionInfo] = []
        for rd in data.get("regions", []):
            cells = [(c[0], c[1]) for c in rd.get("cells", [])]
            shape_cells = [(s[0], s[1]) for s in rd.get("shape", [])]
            regions.append(RegionInfo(
                region_id=rd["region_id"],
                cells=cells,
                area=rd.get("area", len(cells)),
                shape=Shape(cells=frozenset(shape_cells)),
                normalized_shape_key=rd.get("normalized_shape_key", ""),
                matched_shape_name=rd.get("matched_shape_name"),
            ))

        board = self._board_from_regions(puzzle, regions)

        return Solution(
            solved=True,
            board=board,
            regions=regions,
            steps_taken=data.get("steps_taken", 0),
            elapsed_ms=data.get("elapsed_ms", 0),
            rule_results=data.get("rule_results", {}),
        )

    @staticmethod
    def _board_from_regions(puzzle: Puzzle, regions: list[RegionInfo]) -> Board:
        board = Board(puzzle.height, puzzle.width)
        for c in puzzle.cells:
            dst = board.cell(c.row, c.col)
            dst.number = c.number
            dst.symbol = c.symbol
            dst.shape_pattern = c.shape_pattern
            dst.compass = c.compass
            dst.fence_pattern = c.fence_pattern
            dst.blocked = c.blocked
        for region in regions:
            for r, c in region.cells:
                board.cell(r, c).region_id = region.region_id
        return board

    @classmethod
    def supports(cls, puzzle: Puzzle) -> bool:
        return True
