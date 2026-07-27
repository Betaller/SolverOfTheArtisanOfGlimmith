from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Optional


class Direction(Enum):
    UP = auto()
    DOWN = auto()
    LEFT = auto()
    RIGHT = auto()


@dataclass(frozen=True, slots=True)
class CompassClue:
    up: int
    down: int
    left: int
    right: int

    def get(self, d: Direction) -> int:
        match d:
            case Direction.UP: return self.up
            case Direction.DOWN: return self.down
            case Direction.LEFT: return self.left
            case Direction.RIGHT: return self.right


@dataclass(frozen=True, slots=True)
class Shape:
    cells: frozenset[tuple[int, int]]

    @property
    def area(self) -> int:
        return len(self.cells)

    @property
    def bounding_box(self) -> tuple[int, int]:
        if not self.cells:
            return (0, 0)
        max_r = max(r for r, _ in self.cells)
        max_c = max(c for _, c in self.cells)
        return (max_r + 1, max_c + 1)


class EdgeConstraintType(Enum):
    HETEROGENEOUS = "heterogeneous"
    HOMOGENEOUS = "homogeneous"
    INEQUALITY = "inequality"
    DIFFERENCE = "difference"


@dataclass(slots=True)
class EdgeConstraint:
    type: EdgeConstraintType
    value: Optional[int] = None


@dataclass(slots=True)
class Cell:
    row: int
    col: int
    region_id: Optional[int] = None
    number: Optional[int] = None
    symbol: Optional[str] = None
    shape_pattern: Optional[Shape] = None
    compass: Optional[CompassClue] = None
    fence_pattern: Optional[Shape] = None
    blocked: bool = False

    @property
    def assigned(self) -> bool:
        return self.region_id is not None

    @property
    def fillable(self) -> bool:
        return not self.blocked


@dataclass(slots=True)
class Edge:
    r1: int
    c1: int
    r2: int
    c2: int
    is_boundary: bool = False
    constraint: Optional[EdgeConstraint] = None

    def other_end(self, r: int, c: int) -> tuple[int, int]:
        if (r, c) == (self.r1, self.c1):
            return (self.r2, self.c2)
        return (self.r1, self.c1)


@dataclass(slots=True)
class Vertex:
    row: int
    col: int
    watchtower: Optional[int] = None


class Board:
    def __init__(self, height: int, width: int) -> None:
        if height < 2 or width < 2:
            raise ValueError(f"Grid size must be at least 2x2, got {height}x{width}")
        self.height = height
        self.width = width
        self._cells: list[list[Cell]] = [
            [Cell(row=r, col=c) for c in range(width)]
            for r in range(height)
        ]
        self._edges: list[Edge] = []
        self._vertices: list[Vertex] = []
        self.outer_boundaries: list[tuple[int, int, int, int]] = []
        self._build_edges()
        self._build_vertices()

    def _build_edges(self) -> None:
        for r in range(self.height):
            for c in range(self.width - 1):
                self._edges.append(Edge(r1=r, c1=c, r2=r, c2=c + 1))
        for r in range(self.height - 1):
            for c in range(self.width):
                self._edges.append(Edge(r1=r, c1=c, r2=r + 1, c2=c))

    def _build_vertices(self) -> None:
        for r in range(self.height - 1):
            for c in range(self.width - 1):
                self._vertices.append(Vertex(row=r, col=c))

    def cell(self, r: int, c: int) -> Cell:
        return self._cells[r][c]

    def cells(self) -> list[Cell]:
        return [cell for row in self._cells for cell in row]

    def edges(self) -> list[Edge]:
        return self._edges

    def vertices(self) -> list[Vertex]:
        return self._vertices

    def neighbors(self, r: int, c: int) -> list[Cell]:
        result: list[Cell] = []
        for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
            nr, nc = r + dr, c + dc
            if 0 <= nr < self.height and 0 <= nc < self.width:
                result.append(self._cells[nr][nc])
        return result

    def neighbor_positions(self, r: int, c: int) -> list[tuple[int, int]]:
        result: list[tuple[int, int]] = []
        for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
            nr, nc = r + dr, c + dc
            if 0 <= nr < self.height and 0 <= nc < self.width:
                result.append((nr, nc))
        return result

    def edge_between(self, r1: int, c1: int, r2: int, c2: int) -> Optional[Edge]:
        for e in self._edges:
            if (e.r1 == r1 and e.c1 == c1 and e.r2 == r2 and e.c2 == c2) or \
               (e.r1 == r2 and e.c1 == c2 and e.r2 == r1 and e.c2 == c1):
                return e
        return None

    def vertex_at(self, r: int, c: int) -> Optional[Vertex]:
        for v in self._vertices:
            if v.row == r and v.col == c:
                return v
        return None

    def cells_surrounding_vertex(self, vr: int, vc: int) -> list[Cell]:
        return [
            self._cells[vr][vc],
            self._cells[vr][vc + 1],
            self._cells[vr + 1][vc],
            self._cells[vr + 1][vc + 1],
        ]

    def edges_surrounding_vertex(self, vr: int, vc: int) -> list[Edge]:
        edges: list[Edge] = []
        top = self.edge_between(vr, vc, vr, vc + 1)
        bottom = self.edge_between(vr + 1, vc, vr + 1, vc + 1)
        left = self.edge_between(vr, vc, vr + 1, vc)
        right = self.edge_between(vr, vc + 1, vr + 1, vc + 1)
        for e in [top, bottom, left, right]:
            if e is not None:
                edges.append(e)
        return edges

    def clone(self) -> Board:
        b = Board(self.height, self.width)
        for r in range(self.height):
            for c in range(self.width):
                src = self._cells[r][c]
                dst = b._cells[r][c]
                dst.region_id = src.region_id
                dst.number = src.number
                dst.symbol = src.symbol
                dst.shape_pattern = src.shape_pattern
                dst.compass = src.compass
                dst.fence_pattern = src.fence_pattern
                dst.blocked = src.blocked
        for i, src in enumerate(self._edges):
            b._edges[i].is_boundary = src.is_boundary
            b._edges[i].constraint = src.constraint
        for i, src in enumerate(self._vertices):
            b._vertices[i].watchtower = src.watchtower
        return b

    def get_region_cells(self, region_id: int) -> list[Cell]:
        return [c for c in self.cells() if c.region_id == region_id]

    def get_regions(self) -> dict[int, list[Cell]]:
        regions: dict[int, list[Cell]] = {}
        for c in self.cells():
            if c.region_id is not None:
                regions.setdefault(c.region_id, []).append(c)
        return regions

    def unassigned_cells(self) -> list[Cell]:
        return [c for c in self.cells() if c.region_id is None]

    @property
    def is_complete(self) -> bool:
        return all(c.region_id is not None for c in self.cells())
