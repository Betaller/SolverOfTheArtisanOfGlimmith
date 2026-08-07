from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional

from src.models.board import Shape, Board


@dataclass(slots=True)
class RegionInfo:
    region_id: int
    cells: list[tuple[int, int]]
    area: int
    shape: Shape
    normalized_shape_key: str  # canonical form hash
    matched_shape_name: str | None = None  # shape pool match


@dataclass(slots=True)
class Solution:
    board: Board | None = None
    solved: bool = False
    regions: list[RegionInfo] = field(default_factory=list)
    steps_taken: int = 0
    elapsed_ms: int = 0
    error_message: str | None = None
    rule_results: dict[str, bool] = field(default_factory=dict)
    # Which Rust solver module produced this result (aog / rose / pieces /
    # backtrack).  Empty for errors, empty-grid, or timeout placeholders.
    solver: str = ""

    def region_of(self, r: int, c: int) -> RegionInfo | None:
        for reg in self.regions:
            if (r, c) in reg.cells:
                return reg
        return None
