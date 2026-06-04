from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional

from src.models.board import Board, Cell, Edge, Vertex, Shape, EdgeConstraint, CompassClue


RULE_NAMES: dict[str, str] = {
    "shape_pool": "形状池",
    "rose_window": "玫瑰窗",
    "heterogeneous": "异生",
    "homogeneous": "双生",
    "precise": "精确",
    "puzzle_piece": "拼块",
    "mixed": "混合",
    "area": "面积",
    "same": "相同",
    "range": "范围",
    "fence": "围栏",
    "different": "相异",
    "solitary": "独居",
    "block": "方块",
    "non_block": "非方块",
    "differentiation": "差异化",
    "brick": "砖纹",
    "ring": "环纹",
    "inequality": "不等号",
    "difference": "差值",
    "watchtower": "望塔",
    "compass": "罗盘",
}

RULE_IDS: dict[str, str] = {v: k for k, v in RULE_NAMES.items()}


@dataclass(slots=True)
class Rule:
    type: str
    params: dict = field(default_factory=dict)

    @property
    def display_name(self) -> str:
        return RULE_NAMES.get(self.type, self.type)

    @classmethod
    def shape_pool(cls, shapes: list[Shape]) -> Rule:
        return cls(type="shape_pool", params={"shapes": shapes})

    @classmethod
    def rose_window(cls, symbol_types: list[str]) -> Rule:
        return cls(type="rose_window", params={"symbol_types": symbol_types})

    @classmethod
    def heterogeneous(cls) -> Rule:
        return cls(type="heterogeneous")

    @classmethod
    def homogeneous(cls) -> Rule:
        return cls(type="homogeneous")

    @classmethod
    def precise(cls, area: int) -> Rule:
        return cls(type="precise", params={"area": area})

    @classmethod
    def puzzle_piece(cls) -> Rule:
        return cls(type="puzzle_piece")

    @classmethod
    def mixed(cls) -> Rule:
        return cls(type="mixed")

    @classmethod
    def area(cls) -> Rule:
        return cls(type="area")

    @classmethod
    def same(cls) -> Rule:
        return cls(type="same")

    @classmethod
    def range(cls, min_area: int, max_area: int) -> Rule:
        return cls(type="range", params={"min": min_area, "max": max_area})

    @classmethod
    def fence(cls) -> Rule:
        return cls(type="fence")

    @classmethod
    def different(cls) -> Rule:
        return cls(type="different")

    @classmethod
    def solitary(cls) -> Rule:
        return cls(type="solitary")

    @classmethod
    def block(cls) -> Rule:
        return cls(type="block")

    @classmethod
    def non_block(cls) -> Rule:
        return cls(type="non_block")

    @classmethod
    def differentiation(cls) -> Rule:
        return cls(type="differentiation")

    @classmethod
    def brick(cls) -> Rule:
        return cls(type="brick")

    @classmethod
    def ring(cls) -> Rule:
        return cls(type="ring")

    @classmethod
    def inequality(cls) -> Rule:
        return cls(type="inequality")

    @classmethod
    def difference(cls) -> Rule:
        return cls(type="difference")

    @classmethod
    def watchtower(cls) -> Rule:
        return cls(type="watchtower")

    @classmethod
    def compass(cls) -> Rule:
        return cls(type="compass")


CONFLICTING_RULES: list[tuple[str, str]] = [
    ("same", "different"),
    ("block", "non_block"),
    ("precise", "range"),
    ("same", "mixed"),
    ("mixed", "heterogeneous"),
]


def check_rule_conflicts(rules: list[Rule]) -> list[str]:
    warnings: list[str] = []
    active = {r.type for r in rules}
    for a, b in CONFLICTING_RULES:
        if a in active and b in active:
            warnings.append(
                f"规则 '{RULE_NAMES.get(a, a)}' 与 '{RULE_NAMES.get(b, b)}' 互为冲突"
            )
    return warnings


@dataclass(slots=True)
class Puzzle:
    height: int
    width: int
    cells: list[Cell]  # flat list
    edges: list[Edge]
    vertices: list[Vertex]
    outer_boundaries: list[tuple[int, int, int, int]] = field(default_factory=list)
    rules: list[Rule] = field(default_factory=list)
    shape_pool: list[Shape] = field(default_factory=list)

    @classmethod
    def from_board(cls, board: Board, rules: list[Rule] | None = None) -> Puzzle:
        return cls(
            height=board.height,
            width=board.width,
            cells=board.cells(),
            edges=board.edges(),
            vertices=board.vertices(),
            outer_boundaries=list(board.outer_boundaries),
            rules=rules or [],
        )

    def has_rule(self, rule_type: str) -> bool:
        return any(r.type == rule_type for r in self.rules)

    def get_rule(self, rule_type: str) -> Rule | None:
        for r in self.rules:
            if r.type == rule_type:
                return r
        return None
