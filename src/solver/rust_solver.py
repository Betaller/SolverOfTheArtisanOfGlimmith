from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

from src.io.puzzle_codec import puzzle_to_dict
from src.models.board import Board, Shape
from src.models.puzzle import Puzzle
from src.models.solution import Solution, RegionInfo
from src.solver.base import Solver


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

    def solve(self, puzzle: Puzzle, timeout: float = 30.0) -> Solution:
        input_json = json.dumps(puzzle_to_dict(puzzle), ensure_ascii=True)

        try:
            proc = subprocess.run(
                [self._binary],
                input=input_json,
                capture_output=True,
                text=True,
                timeout=timeout,
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
            return Solution(
                solved=False,
                error_message=f"Rust solver exited with code {proc.returncode}: {proc.stderr[:500]}",
            )

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
