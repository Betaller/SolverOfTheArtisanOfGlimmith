from __future__ import annotations

import pytest

from src.models.board import (
    Board, Cell, Edge, EdgeConstraint, EdgeConstraintType,
    Vertex, Shape, CompassClue, Direction,
)


class TestBoardConstruction:
    def test_valid_dimensions(self) -> None:
        b = Board(4, 4)
        assert b.height == 4
        assert b.width == 4

    def test_min_dimensions(self) -> None:
        b = Board(2, 2)
        assert b.height == 2
        assert b.width == 2

    def test_max_dimensions(self) -> None:
        b = Board(16, 16)
        assert b.height == 16
        assert b.width == 16

    def test_valid_large_grid(self) -> None:
        board = Board(50, 50)
        assert board.height == 50 and board.width == 50

    def test_invalid_height_too_small(self) -> None:
        with pytest.raises(ValueError, match="Grid size"):
            Board(1, 4)

    def test_invalid_height_too_small(self) -> None:
        with pytest.raises(ValueError, match="Grid size"):
            Board(1, 4)

    def test_invalid_width_too_small(self) -> None:
        with pytest.raises(ValueError, match="Grid size"):
            Board(4, 1)

    def test_invalid_width_too_small(self) -> None:
        with pytest.raises(ValueError, match="Grid size"):
            Board(4, 1)

    def test_invalid_both_dimensions(self) -> None:
        with pytest.raises(ValueError, match="Grid size"):
            Board(0, 0)

    def test_cells_count(self) -> None:
        b = Board(3, 5)
        assert len(b.cells()) == 15

    def test_edges_count_4x4(self) -> None:
        b = Board(4, 4)
        # horizontal: height * (width - 1) = 4*3 = 12
        # vertical: (height - 1) * width = 3*4 = 12
        # total = 24
        assert len(b.edges()) == 24

    def test_edges_count_2x2(self) -> None:
        b = Board(2, 2)
        assert len(b.edges()) == 4

    def test_vertices_count_4x4(self) -> None:
        b = Board(4, 4)
        # absolute grid corners: (4+1)×(4+1)
        assert len(b.vertices()) == 25

    def test_vertices_count_2x2(self) -> None:
        b = Board(2, 2)
        assert len(b.vertices()) == 9

    def test_vertices_count_1xN(self) -> None:
        # minimum valid: 2x2, so 1xN is not valid
        pass


class TestCell:
    def test_default_cell(self) -> None:
        c = Cell(row=0, col=0)
        assert c.row == 0
        assert c.col == 0
        assert c.region_id is None
        assert c.number is None
        assert c.symbol is None
        assert c.shape_pattern is None
        assert c.compass is None
        assert c.fence_pattern is None
        assert c.assigned is False

    def test_assigned_when_region_id_set(self) -> None:
        c = Cell(row=0, col=0, region_id=1)
        assert c.assigned is True

    def test_assigned_when_region_id_none(self) -> None:
        c = Cell(row=0, col=0, region_id=None)
        assert c.assigned is False

    def test_cell_with_all_fields(self) -> None:
        shape = Shape(cells=frozenset([(0, 0), (0, 1)]))
        compass = CompassClue(up=1, down=2, left=-1, right=-1)
        fence = Shape(cells=frozenset([(0, 0), (0, 1), (1, 0), (1, 1)]))
        c = Cell(
            row=2, col=3, region_id=5, number=4,
            symbol="A", shape_pattern=shape,
            compass=compass, fence_pattern=fence,
        )
        assert c.row == 2
        assert c.col == 3
        assert c.region_id == 5
        assert c.number == 4
        assert c.symbol == "A"
        assert c.shape_pattern == shape
        assert c.compass == compass
        assert c.fence_pattern == fence
        assert c.assigned is True

    def test_cell_mutation(self) -> None:
        c = Cell(row=0, col=0)
        c.region_id = 3
        assert c.region_id == 3
        c.number = 7
        assert c.number == 7
        c.symbol = "B"
        assert c.symbol == "B"
        assert c.assigned is True

    def test_cell_assigned_mutates_with_region_id(self) -> None:
        c = Cell(row=1, col=2)
        assert c.assigned is False
        c.region_id = 0
        assert c.assigned is True
        c.region_id = None
        assert c.assigned is False


class TestEdge:
    def test_default_edge(self) -> None:
        e = Edge(r1=0, c1=0, r2=0, c2=1)
        assert e.r1 == 0
        assert e.c1 == 0
        assert e.r2 == 0
        assert e.c2 == 1
        assert e.is_boundary is False
        assert e.constraint is None

    def test_edge_with_constraint(self) -> None:
        constraint = EdgeConstraint(type=EdgeConstraintType.HETEROGENEOUS)
        e = Edge(r1=0, c1=0, r2=1, c2=0, is_boundary=True, constraint=constraint)
        assert e.is_boundary is True
        assert e.constraint is not None
        assert e.constraint.type == EdgeConstraintType.HETEROGENEOUS

    def test_other_end_first(self) -> None:
        e = Edge(r1=0, c1=0, r2=0, c2=1)
        assert e.other_end(0, 0) == (0, 1)

    def test_other_end_second(self) -> None:
        e = Edge(r1=0, c1=0, r2=0, c2=1)
        assert e.other_end(0, 1) == (0, 0)

    def test_other_end_unrelated(self) -> None:
        e = Edge(r1=2, c1=3, r2=5, c2=7)
        assert e.other_end(5, 7) == (2, 3)
        assert e.other_end(2, 3) == (5, 7)


class TestVertex:
    def test_default_vertex(self) -> None:
        v = Vertex(row=0, col=0)
        assert v.row == 0
        assert v.col == 0
        assert v.watchtower is None

    def test_vertex_with_watchtower(self) -> None:
        v = Vertex(row=1, col=2, watchtower=3)
        assert v.watchtower == 3

    def test_vertex_mutation(self) -> None:
        v = Vertex(row=0, col=0)
        v.watchtower = 4
        assert v.watchtower == 4


class TestCompassClue:
    def test_compass_creation(self) -> None:
        c = CompassClue(up=1, down=2, left=3, right=4)
        assert c.up == 1
        assert c.down == 2
        assert c.left == 3
        assert c.right == 4

    def test_compass_get_up(self) -> None:
        c = CompassClue(up=5, down=6, left=-1, right=-1)
        assert c.get(Direction.UP) == 5

    def test_compass_get_down(self) -> None:
        c = CompassClue(up=5, down=6, left=-1, right=-1)
        assert c.get(Direction.DOWN) == 6

    def test_compass_get_left(self) -> None:
        c = CompassClue(up=-1, down=-1, left=7, right=8)
        assert c.get(Direction.LEFT) == 7

    def test_compass_get_right(self) -> None:
        c = CompassClue(up=-1, down=-1, left=7, right=8)
        assert c.get(Direction.RIGHT) == 8

    def test_compass_negative_values(self) -> None:
        c = CompassClue(up=-1, down=-1, left=-1, right=-1)
        assert c.get(Direction.UP) == -1
        assert c.get(Direction.DOWN) == -1
        assert c.get(Direction.LEFT) == -1
        assert c.get(Direction.RIGHT) == -1


class TestShape:
    def test_shape_creation(self) -> None:
        s = Shape(cells=frozenset([(0, 0), (0, 1), (1, 0)]))
        assert s.area == 3

    def test_bounding_box_single_cell(self) -> None:
        s = Shape(cells=frozenset([(0, 0)]))
        assert s.bounding_box == (1, 1)

    def test_bounding_box_multiple_cells(self) -> None:
        s = Shape(cells=frozenset([(0, 0), (0, 2), (2, 0)]))
        assert s.bounding_box == (3, 3)

    def test_bounding_box_empty(self) -> None:
        s = Shape(cells=frozenset())
        assert s.bounding_box == (0, 0)

    def test_area(self) -> None:
        s = Shape(cells=frozenset([(0, 0), (0, 1), (1, 0), (1, 1)]))
        assert s.area == 4


class TestEdgeConstraint:
    def test_heterogeneous(self) -> None:
        c = EdgeConstraint(type=EdgeConstraintType.HETEROGENEOUS)
        assert c.type == EdgeConstraintType.HETEROGENEOUS
        assert c.value is None

    def test_homogeneous(self) -> None:
        c = EdgeConstraint(type=EdgeConstraintType.HOMOGENEOUS)
        assert c.type == EdgeConstraintType.HOMOGENEOUS

    def test_inequality(self) -> None:
        c = EdgeConstraint(type=EdgeConstraintType.INEQUALITY, value=3)
        assert c.type == EdgeConstraintType.INEQUALITY
        assert c.value == 3

    def test_difference(self) -> None:
        c = EdgeConstraint(type=EdgeConstraintType.DIFFERENCE, value=2)
        assert c.type == EdgeConstraintType.DIFFERENCE
        assert c.value == 2


class TestBoardAccess:
    def test_cell_access(self) -> None:
        b = Board(4, 4)
        c = b.cell(2, 3)
        assert c.row == 2
        assert c.col == 3

    def test_cell_out_of_bounds_raises(self) -> None:
        b = Board(4, 4)
        with pytest.raises(IndexError):
            b.cell(4, 0)
        with pytest.raises(IndexError):
            b.cell(0, 4)

    def test_cells_flat(self) -> None:
        b = Board(2, 3)
        cells = b.cells()
        assert len(cells) == 6
        for r in range(2):
            for c in range(3):
                assert any(cell.row == r and cell.col == c for cell in cells)

    def test_edges_list(self) -> None:
        b = Board(2, 2)
        edges = b.edges()
        assert len(edges) == 4

    def test_vertices_list(self) -> None:
        b = Board(3, 3)
        vertices = b.vertices()
        assert len(vertices) == 16

    def test_neighbors_center(self) -> None:
        b = Board(3, 3)
        neighbors = b.neighbors(1, 1)
        assert len(neighbors) == 4
        positions = {(n.row, n.col) for n in neighbors}
        assert positions == {(0, 1), (1, 0), (1, 2), (2, 1)}

    def test_neighbors_corner(self) -> None:
        b = Board(3, 3)
        neighbors = b.neighbors(0, 0)
        assert len(neighbors) == 2
        positions = {(n.row, n.col) for n in neighbors}
        assert positions == {(0, 1), (1, 0)}

    def test_neighbors_edge(self) -> None:
        b = Board(3, 3)
        neighbors = b.neighbors(0, 1)
        assert len(neighbors) == 3

    def test_neighbor_positions_center(self) -> None:
        b = Board(3, 3)
        positions = b.neighbor_positions(1, 1)
        assert len(positions) == 4
        assert (0, 1) in positions
        assert (1, 0) in positions
        assert (1, 2) in positions
        assert (2, 1) in positions

    def test_neighbor_positions_corner(self) -> None:
        b = Board(3, 3)
        positions = b.neighbor_positions(0, 0)
        assert len(positions) == 2

    def test_edge_between_horizontal(self) -> None:
        b = Board(3, 3)
        e = b.edge_between(0, 0, 0, 1)
        assert e is not None
        assert e.r1 == 0 and e.c1 == 0 and e.r2 == 0 and e.c2 == 1

    def test_edge_between_vertical(self) -> None:
        b = Board(3, 3)
        e = b.edge_between(0, 0, 1, 0)
        assert e is not None
        assert e.r1 == 0 and e.c1 == 0 and e.r2 == 1 and e.c2 == 0

    def test_edge_between_reversed(self) -> None:
        b = Board(3, 3)
        e = b.edge_between(0, 1, 0, 0)
        assert e is not None

    def test_edge_between_nonexistent(self) -> None:
        b = Board(3, 3)
        e = b.edge_between(0, 0, 2, 2)
        assert e is None

    def test_vertex_at_exists(self) -> None:
        b = Board(3, 3)
        v = b.vertex_at(1, 1)
        assert v is not None
        assert v.row == 1
        assert v.col == 1

    def test_vertex_at_not_exists(self) -> None:
        b = Board(3, 3)
        v = b.vertex_at(5, 5)
        assert v is None

    def test_cells_surrounding_vertex(self) -> None:
        b = Board(3, 3)
        # absolute grid corner (2,2) is the corner of cells (1,1)..(2,2)
        cells = b.cells_surrounding_vertex(2, 2)
        assert len(cells) == 4
        positions = {(c.row, c.col) for c in cells}
        assert positions == {(1, 1), (1, 2), (2, 1), (2, 2)}

    def test_edges_surrounding_vertex(self) -> None:
        b = Board(3, 3)
        edges = b.edges_surrounding_vertex(1, 1)
        assert len(edges) == 4


class TestBoardClone:
    def test_clone_structure(self) -> None:
        b = Board(4, 4)
        clone = b.clone()
        assert clone.height == b.height
        assert clone.width == b.width
        assert len(clone.cells()) == len(b.cells())
        assert len(clone.edges()) == len(b.edges())
        assert len(clone.vertices()) == len(b.vertices())

    def test_clone_is_independent(self) -> None:
        b = Board(4, 4)
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        clone = b.clone()
        assert clone.cell(0, 0).region_id == 1
        clone.cell(0, 0).region_id = 2
        assert b.cell(0, 0).region_id == 1

    def test_clone_copies_cell_attributes(self) -> None:
        b = Board(4, 4)
        b.cell(0, 0).number = 4
        b.cell(0, 0).symbol = "A"
        b.cell(0, 0).region_id = 1
        shape = Shape(cells=frozenset([(0, 0), (0, 1)]))
        b.cell(0, 0).shape_pattern = shape
        compass = CompassClue(up=1, down=2, left=-1, right=-1)
        b.cell(0, 0).compass = compass
        fence = Shape(cells=frozenset([(0, 0)]))
        b.cell(0, 0).fence_pattern = fence

        clone = b.clone()
        assert clone.cell(0, 0).number == 4
        assert clone.cell(0, 0).symbol == "A"
        assert clone.cell(0, 0).region_id == 1
        assert clone.cell(0, 0).shape_pattern == shape
        assert clone.cell(0, 0).compass == compass
        assert clone.cell(0, 0).fence_pattern == fence

    def test_clone_copies_edge_boundary(self) -> None:
        b = Board(2, 2)
        e = b.edge_between(0, 0, 0, 1)
        assert e is not None
        e.is_boundary = True
        clone = b.clone()
        cloned_e = clone.edge_between(0, 0, 0, 1)
        assert cloned_e is not None
        assert cloned_e.is_boundary is True

    def test_clone_copies_vertex_watchtower(self) -> None:
        b = Board(3, 3)
        v = b.vertex_at(1, 1)
        assert v is not None
        v.watchtower = 3
        clone = b.clone()
        cloned_v = clone.vertex_at(1, 1)
        assert cloned_v is not None
        assert cloned_v.watchtower == 3

    def test_clone_preserves_outer_boundaries(self) -> None:
        # Bug L5: clone() used to drop outer_boundaries, silently losing
        # pre-drawn outer edges whenever a board was cloned.
        b = Board(3, 3)
        b.outer_boundaries = [(0, 0, 0, 1), (3, 0, 3, 1)]
        clone = b.clone()
        assert clone.outer_boundaries == [(0, 0, 0, 1), (3, 0, 3, 1)]

    def test_clone_outer_boundaries_is_independent(self) -> None:
        b = Board(3, 3)
        b.outer_boundaries = [(0, 0, 0, 1)]
        clone = b.clone()
        clone.outer_boundaries.append((0, 1, 0, 2))
        assert b.outer_boundaries == [(0, 0, 0, 1)]


class TestBoardRegion:
    def test_get_region_cells(self, empty_board_4x4: Board) -> None:
        b = empty_board_4x4
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        b.cell(1, 0).region_id = 2
        cells = b.get_region_cells(1)
        assert len(cells) == 2
        assert {(c.row, c.col) for c in cells} == {(0, 0), (0, 1)}

    def test_get_region_cells_empty(self, empty_board_4x4: Board) -> None:
        b = empty_board_4x4
        cells = b.get_region_cells(99)
        assert cells == []

    def test_get_regions(self, empty_board_4x4: Board) -> None:
        b = empty_board_4x4
        b.cell(0, 0).region_id = 1
        b.cell(0, 1).region_id = 1
        b.cell(1, 0).region_id = 2
        regions = b.get_regions()
        assert len(regions) == 2
        assert len(regions[1]) == 2
        assert len(regions[2]) == 1

    def test_get_regions_ignores_none(self, empty_board_4x4: Board) -> None:
        b = empty_board_4x4
        b.cell(0, 0).region_id = 1
        regions = b.get_regions()
        assert len(regions) == 1

    def test_unassigned_cells(self, empty_board_4x4: Board) -> None:
        b = empty_board_4x4
        b.cell(0, 0).region_id = 1
        b.cell(1, 1).region_id = 2
        unassigned = b.unassigned_cells()
        assert len(unassigned) == 14

    def test_unassigned_cells_all_assigned(self, empty_board_4x4: Board) -> None:
        b = empty_board_4x4
        for r in range(4):
            for c in range(4):
                b.cell(r, c).region_id = 0
        unassigned = b.unassigned_cells()
        assert len(unassigned) == 0

    def test_is_complete_true(self, empty_board_4x4: Board) -> None:
        b = empty_board_4x4
        for r in range(4):
            for c in range(4):
                b.cell(r, c).region_id = 0
        assert b.is_complete is True

    def test_is_complete_false(self, empty_board_4x4: Board) -> None:
        b = empty_board_4x4
        b.cell(0, 0).region_id = 1
        assert b.is_complete is False


class TestDirection:
    def test_direction_values(self) -> None:
        assert Direction.UP.name == "UP"
        assert Direction.DOWN.name == "DOWN"
        assert Direction.LEFT.name == "LEFT"
        assert Direction.RIGHT.name == "RIGHT"

    def test_direction_distinct(self) -> None:
        assert len({Direction.UP, Direction.DOWN, Direction.LEFT, Direction.RIGHT}) == 4


class TestEdgeConstraintType:
    def test_values(self) -> None:
        assert EdgeConstraintType.HETEROGENEOUS.value == "heterogeneous"
        assert EdgeConstraintType.HOMOGENEOUS.value == "homogeneous"
        assert EdgeConstraintType.INEQUALITY.value == "inequality"
        assert EdgeConstraintType.DIFFERENCE.value == "difference"
