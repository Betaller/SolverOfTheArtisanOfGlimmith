from __future__ import annotations

from typing import Optional

from PySide6.QtCore import Qt, QRectF, QPointF, Signal, QSize
from PySide6.QtGui import (
    QPainter, QPen, QBrush, QColor, QFont, QMouseEvent, QKeyEvent,
    QPaintEvent, QWheelEvent, QAction,
)
from PySide6.QtWidgets import QWidget, QMenu

from src.models.board import Board, Cell, Edge, EdgeConstraintType, Vertex, Shape, CompassClue
from src.ui import theme as _ui_theme


MODE_CURSORS = {
    "select": Qt.CursorShape.ArrowCursor,
    "boundary": Qt.CursorShape.CrossCursor,
    "block": Qt.CursorShape.CrossCursor,
    "number": Qt.CursorShape.IBeamCursor,
    "symbol": Qt.CursorShape.IBeamCursor,
    "compass": Qt.CursorShape.CrossCursor,
    "watchtower": Qt.CursorShape.CrossCursor,
}


class GridWidget(QWidget):
    cell_clicked = Signal(int, int)
    edge_clicked = Signal(int, int, int, int)
    vertex_clicked = Signal(int, int)
    mode_changed = Signal(str)
    status_message = Signal(str)
    board_modified = Signal()

    MODE_SELECT = "select"
    MODE_BOUNDARY = "boundary"
    MODE_NUMBER = "number"
    MODE_SYMBOL = "symbol"
    MODE_COMPASS = "compass"
    MODE_WATCHTOWER = "watchtower"
    MODE_BLOCK = "block"

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.board: Board | None = None
        self._mode = self.MODE_SELECT
        self._cell_size = 60
        self._padding = 24
        self._selected_cell: tuple[int, int] | None = None
        self._selected_edge: tuple[int, int, int, int] | None = None
        self._selected_vertex: tuple[int, int] | None = None
        self._hover_cell: tuple[int, int] | None = None
        self._hover_vertex: tuple[int, int] | None = None
        self._hover_edge: tuple[int, int, int, int] | None = None
        self._region_colors: dict[int, QColor] = {}
        self._current_symbol: str | None = None
        self._current_number: int | None = None
        self._current_compass: CompassClue | None = None

        self._boundary_start_vertex: tuple[int, int] | None = None
        self._boundary_dragging: bool = False
        self._last_boundary_vertex: tuple[int, int] | None = None
        self._outer_boundaries: set[tuple[int, int, int, int]] = set()

        self._block_dragging: bool = False
        self._block_paint_blocked: bool = True

        self._inline_number: str = ""
        self._overlay_rules: list[str] = []
        self._overlay_shapes: list[Shape] = []

        self.setMouseTracking(True)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self.setMinimumSize(200, 200)
        self.setCursor(MODE_CURSORS[self._mode])

    def set_board(self, board: Board) -> None:
        self.board = board
        self._region_colors.clear()
        self._selected_cell = None
        self._selected_edge = None
        self._selected_vertex = None
        self._hover_cell = None
        self._hover_vertex = None
        self._hover_edge = None
        self._boundary_start_vertex = None
        self._boundary_dragging = False
        self._last_boundary_vertex = None
        self._outer_boundaries.clear()
        if board is not None:
            for key in board.outer_boundaries:
                self._outer_boundaries.add(key)
            self._cache_rects()
        self._block_dragging = False
        self._inline_number = ""
        self.update()

    def sizeHint(self) -> QSize:
        if self.board is None:
            return QSize(400, 300)
        w = self._padding * 2 + self.board.width * self._cell_size
        h = self._padding * 2 + self.board.height * self._cell_size
        return QSize(w, h)

    def minimumSizeHint(self) -> QSize:
        return self.sizeHint()

    def _cache_rects(self) -> None:
        """Precompute hit-test rects for O(1) lookup instead of O(H×W) scan."""
        if self.board is None:
            return
        h, w = self.board.height, self.board.width
        pad, sz = self._padding, self._cell_size
        self._cell_rects: dict[tuple[int, int], QRectF] = {}
        self._vertex_positions: dict[tuple[int, int], QPointF] = {}
        self._edge_rects: dict[tuple[int, int, int, int], tuple[float, float, float, float]] = {}

        for r in range(h):
            for c in range(w):
                self._cell_rects[(r, c)] = QRectF(pad + c * sz, pad + r * sz, sz, sz)

        for r in range(h + 1):
            for c in range(w + 1):
                self._vertex_positions[(r, c)] = QPointF(pad + c * sz, pad + r * sz)

        for e in self.board.edges():
            self._edge_rects[(e.r1, e.c1, e.r2, e.c2)] = self._edge_endpoints(e)

    def set_mode(self, mode: str) -> None:
        self._mode = mode
        self.setCursor(MODE_CURSORS.get(mode, Qt.CursorShape.ArrowCursor))
        self._boundary_start_vertex = None
        self._boundary_dragging = False
        self._last_boundary_vertex = None
        self._block_dragging = False
        self._inline_number = ""
        self.mode_changed.emit(mode)
        self.update()

    def set_symbol(self, symbol: str | None) -> None:
        self._current_symbol = symbol

    def set_number(self, number: int | None) -> None:
        self._current_number = number

    def set_compass(self, compass: CompassClue | None) -> None:
        self._current_compass = compass

    def set_overlay_info(self, rules: list[str], shapes: list[Shape]) -> None:
        self._overlay_rules = list(rules)
        self._overlay_shapes = list(shapes)
        self.update()

    def refresh(self) -> None:
        self._region_colors.clear()
        self.update()

    def _get_color(self, region_id: int | None) -> QColor:
        if region_id is None:
            return QColor(_ui_theme.colors.cell_bg_null)
        if region_id not in self._region_colors:
            color = _ui_theme.REGION_COLORS[region_id % len(_ui_theme.REGION_COLORS)]
            self._region_colors[region_id] = QColor(color)
        return self._region_colors[region_id]

    def _cell_rect(self, r: int, c: int) -> QRectF:
        x = self._padding + c * self._cell_size
        y = self._padding + r * self._cell_size
        return QRectF(x, y, self._cell_size, self._cell_size)

    def _vertex_pos(self, vr: int, vc: int) -> tuple[float, float]:
        x = self._padding + vc * self._cell_size
        y = self._padding + vr * self._cell_size
        return (x, y)

    def _hit_test_cell(self, pos: QPointF) -> tuple[int, int] | None:
        if self.board is None:
            return None
        if not hasattr(self, '_cell_rects') or not self._cell_rects:
            return None
        for (r, c), rect in self._cell_rects.items():
            if rect.contains(pos):
                return (r, c)
        return None

    def _hit_test_vertex(self, pos: QPointF) -> tuple[int, int] | None:
        if self.board is None:
            return None
        if not hasattr(self, '_vertex_positions') or not self._vertex_positions:
            return None
        threshold = max(10, self._cell_size // 7)
        for (r, c), pt in self._vertex_positions.items():
            if abs(pos.x() - pt.x()) < threshold and abs(pos.y() - pt.y()) < threshold:
                return (r, c)
        return None

    def _hit_test_edge(self, pos: QPointF) -> tuple[int, int, int, int] | None:
        if self.board is None:
            return None
        if not hasattr(self, '_edge_rects') or not self._edge_rects:
            return None
        threshold = max(10, self._cell_size // 7)
        best: tuple[float, tuple[int, int, int, int]] | None = None
        for key, (x1, y1, x2, y2) in self._edge_rects.items():
            dx = x2 - x1
            dy = y2 - y1
            length_sq = dx * dx + dy * dy
            if length_sq == 0:
                continue
            t_val = ((pos.x() - x1) * dx + (pos.y() - y1) * dy) / length_sq
            t_val = max(0.0, min(1.0, t_val))
            px = x1 + t_val * dx
            py = y1 + t_val * dy
            dist = ((pos.x() - px) ** 2 + (pos.y() - py) ** 2) ** 0.5
            if dist < threshold:
                if best is None or dist < best[0]:
                    best = (dist, key)
        return best[1] if best is not None else None

    def _edge_endpoints(self, e: Edge) -> tuple[float, float, float, float]:
        pad = self._padding
        sz = self._cell_size
        if e.r1 == e.r2:
            cx = pad + (e.c1 + 1) * sz
            return (cx, pad + e.r1 * sz, cx, pad + (e.r1 + 1) * sz)
        else:
            ry = pad + (e.r1 + 1) * sz
            return (pad + e.c1 * sz, ry, pad + (e.c1 + 1) * sz, ry)

    def _vertices_adjacent(self, v1: tuple[int, int], v2: tuple[int, int]) -> bool:
        r1, c1 = v1
        r2, c2 = v2
        dr = abs(r1 - r2)
        dc = abs(c1 - c2)
        return (dr == 1 and dc == 0) or (dr == 0 and dc == 1)

    def _outer_key(self, v1: tuple[int, int], v2: tuple[int, int]) -> tuple[int, int, int, int] | None:
        r1, c1 = v1
        r2, c2 = v2
        if not (abs(r1 - r2) + abs(c1 - c2) == 1):
            return None
        if r1 == r2:
            c = min(c1, c2)
            if r1 == 0 or r1 == self.board.height:
                return (r1, c, r1, c + 1)
        if c1 == c2:
            r = min(r1, r2)
            if c1 == 0 or c1 == self.board.width:
                return (r, c1, r + 1, c1)
        return None

    def _vertices_to_edge(self, v1: tuple[int, int], v2: tuple[int, int]) -> Edge | None:
        if self.board is None:
            return None
        r1, c1 = v1
        r2, c2 = v2
        if r1 == r2 and abs(c1 - c2) == 1:
            c = min(c1, c2)
            if 0 < r1 <= self.board.height - 1:
                return self.board.edge_between(r1 - 1, c, r1, c)
            return None
        if c1 == c2 and abs(r1 - r2) == 1:
            r = min(r1, r2)
            if 0 < c1 <= self.board.width - 1:
                return self.board.edge_between(r, c1 - 1, r, c1)
            return None
        return None

    def _clear_cell(self, cell: Cell) -> None:
        cell.number = None
        cell.symbol = None
        cell.compass = None
        cell.shape_pattern = None
        cell.fence_pattern = None
        cell.region_id = None

    def _cell_context_menu(self, pos: QPointF, r: int, c: int) -> None:
        cell = self.board.cell(r, c)
        menu = QMenu(self)

        if cell.blocked:
            act_unblock = menu.addAction("取消障碍")
            act_unblock.triggered.connect(lambda: self._toggle_blocked(r, c))
        else:
            act_block = menu.addAction("设为障碍格")
            act_block.triggered.connect(lambda: self._toggle_blocked(r, c))

        menu.addSeparator()

        act_clear_num = menu.addAction("清除数字")
        act_clear_num.setEnabled(cell.number is not None)
        act_clear_num.triggered.connect(lambda: self._set_cell_attr(r, c, "number", None))

        act_clear_sym = menu.addAction("清除符号")
        act_clear_sym.setEnabled(cell.symbol is not None)
        act_clear_sym.triggered.connect(lambda: self._set_cell_attr(r, c, "symbol", None))

        act_clear_all = menu.addAction("清除全部")
        act_clear_all.setEnabled(not cell.blocked and (
            cell.number is not None or cell.symbol is not None or
            cell.compass is not None or cell.shape_pattern is not None
        ))
        act_clear_all.triggered.connect(lambda: self._clear_cell_properties(r, c))

        menu.addSeparator()

        act_toggle_boundary = menu.addAction("切换此格边框")
        act_toggle_boundary.triggered.connect(lambda: self._toggle_cell_boundary(r, c))

        menu.exec(self.mapToGlobal(pos.toPoint()))

    def _edge_context_menu(self, pos: QPointF, r1: int, c1: int, r2: int, c2: int) -> None:
        e = self.board.edge_between(r1, c1, r2, c2)
        if e is None:
            return
        menu = QMenu(self)

        act_toggle = menu.addAction("切换分割线")
        act_toggle.triggered.connect(lambda: self._toggle_edge_boundary(r1, c1, r2, c2))
        menu.addSeparator()

        act_hetero = menu.addAction("设异生 (≠)")
        act_hetero.setChecked(e.constraint is not None and e.constraint.type == EdgeConstraintType.HETEROGENEOUS)
        act_hetero.triggered.connect(lambda: self._set_edge_constraint(r1, c1, r2, c2, EdgeConstraintType.HETEROGENEOUS))

        act_homo = menu.addAction("设双生 (=)")
        act_homo.setChecked(e.constraint is not None and e.constraint.type == EdgeConstraintType.HOMOGENEOUS)
        act_homo.triggered.connect(lambda: self._set_edge_constraint(r1, c1, r2, c2, EdgeConstraintType.HOMOGENEOUS))

        act_ineq = menu.addAction("设不等号 (箭头)")
        act_ineq.triggered.connect(lambda: self._set_edge_constraint(r1, c1, r2, c2, EdgeConstraintType.INEQUALITY))

        act_diff = menu.addAction("设差值")
        act_diff.triggered.connect(lambda: self._set_edge_constraint(r1, c1, r2, c2, EdgeConstraintType.DIFFERENCE, 1))

        if e.constraint is not None:
            menu.addSeparator()
            act_clear = menu.addAction("清除约束")
            act_clear.triggered.connect(lambda: self._clear_edge_constraint(r1, c1, r2, c2))

        menu.exec(self.mapToGlobal(pos.toPoint()))

    def _vertex_context_menu(self, pos: QPointF, r: int, c: int) -> None:
        v = self.board.vertex_at(r, c)
        if v is None:
            return
        menu = QMenu(self)

        act_clear = menu.addAction("清除望塔值")
        act_clear.setEnabled(v.watchtower is not None)
        act_clear.triggered.connect(lambda: self._clear_watchtower(r, c))
        menu.exec(self.mapToGlobal(pos.toPoint()))

    def _paint_blocked(self, r: int, c: int, blocked: bool) -> None:
        cell = self.board.cell(r, c)
        if cell.blocked != blocked:
            cell.blocked = blocked
            if blocked:
                self._clear_cell(cell)
            self.board_modified.emit()
        self.update()

    def _toggle_blocked(self, r: int, c: int) -> None:
        cell = self.board.cell(r, c)
        cell.blocked = not cell.blocked
        if cell.blocked:
            self._clear_cell(cell)
        self._selected_cell = (r, c)
        self.board_modified.emit()
        self.update()

    def _set_cell_attr(self, r: int, c: int, attr: str, value) -> None:
        cell = self.board.cell(r, c)
        setattr(cell, attr, value)
        self.update()

    def _clear_cell_properties(self, r: int, c: int) -> None:
        cell = self.board.cell(r, c)
        if not cell.blocked:
            cell.number = None
            cell.symbol = None
            cell.compass = None
            cell.shape_pattern = None
            cell.fence_pattern = None
            cell.region_id = None
            self.update()

    def _toggle_cell_boundary(self, r: int, c: int) -> None:
        for e in self.board.edges():
            if (e.r1 == r and e.c1 == c) or (e.r2 == r and e.c2 == c):
                e.is_boundary = not e.is_boundary
        self.update()

    def _toggle_edge_boundary(self, r1: int, c1: int, r2: int, c2: int) -> None:
        e = self.board.edge_between(r1, c1, r2, c2)
        if e is not None:
            e.is_boundary = not e.is_boundary
            self.board_modified.emit()
            self.update()

    def _set_edge_constraint(self, r1: int, c1: int, r2: int, c2: int,
                             ctype: EdgeConstraintType, value: int | None = None) -> None:
        e = self.board.edge_between(r1, c1, r2, c2)
        if e is not None:
            e.constraint = EdgeConstraint(type=ctype, value=value)
            self.edge_clicked.emit(r1, c1, r2, c2)
            self.update()

    def _clear_edge_constraint(self, r1: int, c1: int, r2: int, c2: int) -> None:
        e = self.board.edge_between(r1, c1, r2, c2)
        if e is not None:
            e.constraint = None
            self.edge_clicked.emit(r1, c1, r2, c2)
            self.update()

    def _clear_watchtower(self, r: int, c: int) -> None:
        v = self.board.vertex_at(r, c)
        if v is not None:
            v.watchtower = None
            self.update()

    def mousePressEvent(self, event: QMouseEvent) -> None:
        if self.board is None:
            return
        pos = event.position()

        vertex = self._hit_test_vertex(pos)
        edge = self._hit_test_edge(pos)
        cell = self._hit_test_cell(pos)

        if event.button() == Qt.MouseButton.RightButton:
            if self._mode == self.MODE_BLOCK:
                if cell is not None:
                    self._block_dragging = True
                    self._block_paint_blocked = False
                    self._paint_blocked(cell[0], cell[1], False)
                    self._selected_cell = cell
                    self.update()
                return
            if vertex is not None:
                vr, vc = vertex[0] - 1, vertex[1] - 1
                self._vertex_context_menu(pos, vr, vc)
            elif edge is not None:
                self._edge_context_menu(pos, *edge)
            elif cell is not None:
                self._cell_context_menu(pos, cell[0], cell[1])
            return

        if event.button() != Qt.MouseButton.LeftButton:
            return

        if self._mode == self.MODE_BOUNDARY:
            if vertex is not None:
                # Single click on a vertex: toggle the edge to an adjacent vertex
                if edge is not None:
                    self._toggle_edge_boundary(*edge)
                    self._selected_edge = edge
                    self._selected_vertex = None
                    self._selected_cell = None
                    self.update()
                    return
                # Start dragging from vertex
                self._boundary_dragging = True
                self._last_boundary_vertex = vertex
                self._selected_vertex = vertex
                self._boundary_start_vertex = None
                self.update()
            elif edge is not None:
                # Click directly on edge to toggle
                self._toggle_edge_boundary(*edge)
                self._selected_edge = edge
                self._selected_vertex = None
                self._selected_cell = None
                self.update()
            return

        if self._mode == self.MODE_WATCHTOWER:
            if vertex is not None:
                # Map grid vertex position → Board vertex (offset by -1)
                vr, vc = vertex[0] - 1, vertex[1] - 1
                v = self.board.vertex_at(vr, vc)
                if v is not None:
                    val = self._current_number
                    if val is not None and 1 <= val <= 4:
                        v.watchtower = val
                    self._selected_vertex = vertex
                    self._selected_cell = None
                    self._selected_edge = None
                    self.vertex_clicked.emit(vr, vc)
                    self.update()
            return

        if self._mode == self.MODE_BLOCK:
            if cell is not None:
                self._block_dragging = True
                self._block_paint_blocked = True
                self._paint_blocked(cell[0], cell[1], True)
                self._selected_cell = cell
                self.update()
            return

        if self._mode == self.MODE_NUMBER and cell is not None:
            c_obj = self.board.cell(cell[0], cell[1])
            if self._current_number is not None:
                c_obj.number = self._current_number
            self._selected_cell = cell
            self._selected_edge = None
            self._selected_vertex = None
            self._inline_number = ""
            self.setFocus()
            self.update()
            self.cell_clicked.emit(cell[0], cell[1])
            return

        if self._mode == self.MODE_SYMBOL and cell is not None:
            c_obj = self.board.cell(cell[0], cell[1])
            c_obj.symbol = self._current_symbol
            self._selected_cell = cell
            self._selected_edge = None
            self._selected_vertex = None
            self.update()
            self.cell_clicked.emit(cell[0], cell[1])
            return

        if self._mode == self.MODE_COMPASS and cell is not None:
            c_obj = self.board.cell(cell[0], cell[1])
            c_obj.compass = self._current_compass
            self._selected_cell = cell
            self._selected_edge = None
            self._selected_vertex = None
            self.update()
            self.cell_clicked.emit(cell[0], cell[1])
            return

        if self._mode == self.MODE_SELECT:
            # Vertex first (more specific than edge), then edge, then cell
            if vertex is not None:
                self._selected_vertex = vertex
                self._selected_cell = None
                self._selected_edge = None
                self.vertex_clicked.emit(vertex[0] - 1, vertex[1] - 1)
            elif edge is not None:
                self._selected_edge = edge
                self._selected_cell = None
                self._selected_vertex = None
                self.edge_clicked.emit(edge[0], edge[1], edge[2], edge[3])
            elif cell is not None:
                self._selected_cell = cell
                self._selected_edge = None
                self._selected_vertex = None
                self.setFocus()
                self.cell_clicked.emit(cell[0], cell[1])
            self.update()

    def _handle_boundary_draw(self, vertex: tuple[int, int]) -> None:
        if self._boundary_start_vertex is None:
            self._boundary_start_vertex = vertex
            self._selected_vertex = vertex
            self.update()
            return

        start = self._boundary_start_vertex
        if start == vertex:
            self._boundary_start_vertex = None
            self._selected_vertex = None
            self.update()
            return

        if self._vertices_adjacent(start, vertex):
            e = self._vertices_to_edge(start, vertex)
            if e is not None:
                e.is_boundary = not e.is_boundary
                self.edge_clicked.emit(e.r1, e.c1, e.r2, e.c2)
            else:
                okey = self._outer_key(start, vertex)
                if okey is not None:
                    if okey in self._outer_boundaries:
                        self._outer_boundaries.discard(okey)
                        if self.board is not None:
                            self.board.outer_boundaries = [k for k in self.board.outer_boundaries if k != okey]
                    else:
                        self._outer_boundaries.add(okey)
                        if self.board is not None:
                            self.board.outer_boundaries.append(okey)

        self._boundary_start_vertex = None
        self._selected_vertex = None
        self.update()

    def mouseMoveEvent(self, event: QMouseEvent) -> None:
        if self.board is None:
            return
        pos = event.position()

        if self._mode == self.MODE_BLOCK and self._block_dragging:
            cell = self._hit_test_cell(pos)
            if cell is not None:
                self._paint_blocked(cell[0], cell[1], self._block_paint_blocked)
                self._hover_cell = cell
                self.status_message.emit(f"障碍模式: 单元格 ({cell[0]}, {cell[1]})")
            return

        if self._mode == self.MODE_BOUNDARY and self._boundary_dragging and self._last_boundary_vertex is not None:
            v = self._hit_test_vertex(pos)
            if v is not None and v != self._last_boundary_vertex and self._vertices_adjacent(self._last_boundary_vertex, v):
                e = self._vertices_to_edge(self._last_boundary_vertex, v)
                if e is not None:
                    e.is_boundary = not e.is_boundary
                    self.edge_clicked.emit(e.r1, e.c1, e.r2, e.c2)
                else:
                    okey = self._outer_key(self._last_boundary_vertex, v)
                    if okey is not None:
                        if okey in self._outer_boundaries:
                            self._outer_boundaries.discard(okey)
                            if self.board is not None:
                                self.board.outer_boundaries = [k for k in self.board.outer_boundaries if k != okey]
                        else:
                            self._outer_boundaries.add(okey)
                            if self.board is not None:
                                self.board.outer_boundaries.append(okey)
                self.board_modified.emit()
                self._last_boundary_vertex = v
                self._selected_vertex = v
                self.update()
            elif v is not None and v != self._hover_vertex:
                self._hover_vertex = v
                self._hover_cell = None
                self.update()
            if v is not None:
                self.status_message.emit(f"边框模式: 顶点 ({v[0]}, {v[1]})")

        elif self._mode == self.MODE_WATCHTOWER or (self._mode == self.MODE_BOUNDARY and not self._boundary_dragging):
            v = self._hit_test_vertex(pos)
            if v != self._hover_vertex:
                self._hover_vertex = v
                self._hover_cell = None
                self.update()
            if v is not None:
                self.status_message.emit(f"顶点 ({v[0]}, {v[1]})")
            elif self._mode != self.MODE_BOUNDARY:
                cell = self._hit_test_cell(pos)
                if cell is not None:
                    self.status_message.emit(f"单元格 ({cell[0]}, {cell[1]})")
        else:
            # SELECT / NUMBER / SYMBOL / COMPASS modes: check vertex → edge → cell
            v = self._hit_test_vertex(pos)
            e = self._hit_test_edge(pos)
            changed = False
            if v is not None:
                if v != self._hover_vertex:
                    self._hover_vertex = v
                    self._hover_cell = None
                    self._hover_edge = None
                    changed = True
                self.status_message.emit(f"顶点 ({v[0]}, {v[1]})")
            elif e is not None:
                if e != self._hover_edge:
                    self._hover_edge = e
                    self._hover_cell = None
                    self._hover_vertex = None
                    changed = True
                self.status_message.emit(f"边框 ({e[0]},{e[1]})-({e[2]},{e[3]})")
            else:
                cell = self._hit_test_cell(pos)
                if self._hover_edge is not None or self._hover_vertex is not None:
                    changed = True
                self._hover_edge = None
                self._hover_vertex = None
                if cell != self._hover_cell:
                    self._hover_cell = cell
                    changed = True
                if cell is not None:
                    c_obj = self.board.cell(cell[0], cell[1])
                    extras = []
                    if c_obj.number is not None:
                        extras.append(f"#{c_obj.number}")
                    if c_obj.symbol is not None:
                        extras.append(f"符号:{c_obj.symbol}")
                    if c_obj.blocked:
                        extras.append("障碍")
                    suffix = f" [{' '.join(extras)}]" if extras else ""
                    self.status_message.emit(f"单元格 ({cell[0]}, {cell[1]}){suffix}")
                elif not self._hover_vertex:
                    self.status_message.emit("")
            if changed:
                self.update()

    def keyPressEvent(self, event: QKeyEvent) -> None:
        if self.board is None:
            return

        key = event.key()
        text = event.text()

        if key == Qt.Key.Key_Escape:
            self._selected_cell = None
            self._selected_edge = None
            self._selected_vertex = None
            self._inline_number = ""
            self.update()
            return

        if key == Qt.Key.Key_Delete or key == Qt.Key.Key_Backspace:
            if self._selected_cell is not None:
                r, c = self._selected_cell
                cell = self.board.cell(r, c)
                if not cell.blocked:
                    cell.number = None
                    cell.symbol = None
                    cell.compass = None
                    self._inline_number = ""
                    self.update()
                return

        if self._mode == self.MODE_NUMBER and self._selected_cell is not None:
            if text.isdigit():
                self._inline_number += text
                r, c = self._selected_cell
                cell = self.board.cell(r, c)
                cell.number = int(self._inline_number)
                self.update()
                return
            elif key == Qt.Key.Key_Return or key == Qt.Key.Key_Enter:
                if self._inline_number:
                    r, c = self._selected_cell
                    cell = self.board.cell(r, c)
                    cell.number = int(self._inline_number)
                    self._inline_number = ""
                    self._move_selection(0, 1)
                    self.update()
                return

        if self._selected_cell is not None:
            if text.isdigit():
                r, c = self._selected_cell
                cell = self.board.cell(r, c)
                if not cell.blocked:
                    cell.number = int(text)
                    self._inline_number = ""
                    self.update()
                return

        if key == Qt.Key.Key_Up:
            self._move_selection(-1, 0)
        elif key == Qt.Key.Key_Down:
            self._move_selection(1, 0)
        elif key == Qt.Key.Key_Left:
            self._move_selection(0, -1)
        elif key == Qt.Key.Key_Right:
            self._move_selection(0, 1)
        else:
            super().keyPressEvent(event)

    def _move_selection(self, dr: int, dc: int) -> None:
        if self.board is None:
            return
        if self._selected_cell is not None:
            r, c = self._selected_cell
            nr = max(0, min(self.board.height - 1, r + dr))
            nc = max(0, min(self.board.width - 1, c + dc))
            self._selected_cell = (nr, nc)
            self._selected_edge = None
            self._selected_vertex = None
            self.cell_clicked.emit(nr, nc)
            self.update()
        elif self._selected_vertex is not None:
            r, c = self._selected_vertex
            nr = max(0, min(self.board.height, r + dr))
            nc = max(0, min(self.board.width, c + dc))
            self._selected_vertex = (nr, nc)
            self.update()

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:
        if self._block_dragging:
            self._block_dragging = False
        if event.button() == Qt.MouseButton.LeftButton and self._boundary_dragging:
            self._boundary_dragging = False
            self._last_boundary_vertex = None
            self.update()

    def wheelEvent(self, event: QWheelEvent) -> None:
        if event.angleDelta().y() > 0:
            self._cell_size = min(120, self._cell_size + 5)
        else:
            self._cell_size = max(15, self._cell_size - 5)
        self._cache_rects()
        self.update()

    def paintEvent(self, event: QPaintEvent) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        if self.board is None:
            painter.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "未加载谜题")
            return

        painter.fillRect(self.rect(), QColor(_ui_theme.colors.grid_bg))

        self._draw_cells(painter)
        self._draw_selection(painter)
        self._draw_boundary_edges(painter)
        self._draw_grid_lines(painter)
        self._draw_edge_constraints(painter)
        self._draw_vertices(painter)
        self._draw_clues(painter)
        self._draw_rule_overlay(painter)

    def _is_same_region_internal(self, e: Edge) -> bool:
        """True when both cells are assigned to the same region.

        Such edges are not drawn as grid lines so each region renders as one
        contiguous colour block instead of a patchwork of bordered cells.
        """
        c1 = self.board.cell(e.r1, e.c1)
        c2 = self.board.cell(e.r2, e.c2)
        return (not c1.blocked and not c2.blocked
                and c1.region_id is not None
                and c1.region_id == c2.region_id)

    def _draw_cells(self, painter: QPainter) -> None:
        for r in range(self.board.height):
            for c in range(self.board.width):
                cell = self.board.cell(r, c)
                rect = self._cell_rect(r, c)

                if cell.blocked:
                    painter.fillRect(rect, QColor(_ui_theme.colors.cell_blocked_bg))
                    painter.setPen(QPen(QColor(_ui_theme.colors.cell_blocked_border), 1))
                    painter.drawRect(rect)
                    painter.setPen(QPen(QColor(_ui_theme.colors.cell_blocked_x), 2))
                    painter.drawLine(rect.topLeft(), rect.bottomRight())
                    painter.drawLine(rect.topRight(), rect.bottomLeft())
                    continue

                color = self._get_color(cell.region_id)
                painter.fillRect(rect, color)

    def _draw_outer_edge(self, painter: QPainter, key: tuple[int, int, int, int]) -> None:
        r1, c1, r2, c2 = key
        pad, sz = self._padding, self._cell_size
        if r1 == r2:
            x1 = pad + c1 * sz
            x2 = pad + c2 * sz
            y = pad + r1 * sz
            painter.drawLine(QPointF(x1, y), QPointF(x2, y))
        else:
            x = pad + c1 * sz
            y1 = pad + r1 * sz
            y2 = pad + r2 * sz
            painter.drawLine(QPointF(x, y1), QPointF(x, y2))

    def _is_auto_boundary(self, e: Edge) -> bool:
        """Edge that separates a fillable cell from a blocked cell.

        Such borders are always drawn, even when the puzzle data does not mark
        the edge as a pre-drawn boundary, so the playable area's outline stays
        visible on irregular boards.
        """
        c1 = self.board.cell(e.r1, e.c1)
        c2 = self.board.cell(e.r2, e.c2)
        return c1.blocked != c2.blocked

    def _draw_boundary_edges(self, painter: QPainter) -> None:
        pen = QPen(QColor(_ui_theme.colors.boundary_edge), 6)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        painter.setPen(pen)
        for e in self.board.edges():
            if e.is_boundary or self._is_auto_boundary(e):
                x1, y1, x2, y2 = self._edge_endpoints(e)
                painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))
        for key in self._outer_boundaries:
            self._draw_outer_edge(painter, key)

        painter.setPen(QPen(QColor(_ui_theme.colors.boundary_highlight), 2.5))
        for e in self.board.edges():
            if e.is_boundary or self._is_auto_boundary(e):
                x1, y1, x2, y2 = self._edge_endpoints(e)
                painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))
        for key in self._outer_boundaries:
            self._draw_outer_edge(painter, key)

    def _draw_grid_lines(self, painter: QPainter) -> None:
        painter.setPen(QPen(QColor(_ui_theme.colors.grid_line), 1))
        for e in self.board.edges():
            if self._is_same_region_internal(e):
                continue
            x1, y1, x2, y2 = self._edge_endpoints(e)
            painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))
        # outer perimeter so the board outline stays visible even when the
        # puzzle carries no explicit outer_boundaries
        pad, sz = self._padding, self._cell_size
        painter.drawRect(QRectF(pad, pad,
                                self.board.width * sz, self.board.height * sz))

    def _draw_edge_constraints(self, painter: QPainter) -> None:
        for e in self.board.edges():
            if e.constraint is None:
                continue
            x1, y1, x2, y2 = self._edge_endpoints(e)
            mx, my = (x1 + x2) / 2, (y1 + y2) / 2
            sz = max(18, self._cell_size // 4)
            bg = QRectF(mx - sz * 0.6, my - sz * 0.5, sz * 1.2, sz)
            painter.setPen(Qt.PenStyle.NoPen)
            painter.setBrush(QBrush(QColor(_ui_theme.colors.edge_constr_bg)))
            painter.drawRoundedRect(bg, 4, 4)
            painter.setPen(QPen(QColor(_ui_theme.colors.edge_constr_border), 1))
            painter.drawRoundedRect(bg, 4, 4)

            font = QFont("Segoe UI", self._cell_size // 6, QFont.Weight.Bold)
            painter.setFont(font)
            painter.setPen(QPen(QColor(_ui_theme.colors.edge_constr_text)))

            ct = e.constraint.type
            if ct == EdgeConstraintType.HETEROGENEOUS:
                painter.drawText(QRectF(mx - 14, my - 10, 28, 20),
                                 Qt.AlignmentFlag.AlignCenter, "≠")
            elif ct == EdgeConstraintType.HOMOGENEOUS:
                painter.drawText(QRectF(mx - 14, my - 10, 28, 20),
                                 Qt.AlignmentFlag.AlignCenter, "=")
            elif ct == EdgeConstraintType.INEQUALITY:
                rev = e.constraint.value == 1 if e.constraint is not None else False
                if e.c1 == e.c2:
                    if rev:
                        arrow = "↓" if e.r1 < e.r2 else "↑"
                    else:
                        arrow = "↑" if e.r1 < e.r2 else "↓"
                else:
                    if rev:
                        arrow = "→" if e.c1 < e.c2 else "←"
                    else:
                        arrow = "←" if e.c1 < e.c2 else "→"
                painter.drawText(QRectF(mx - 14, my - 10, 28, 20),
                                 Qt.AlignmentFlag.AlignCenter, arrow)
            elif ct == EdgeConstraintType.DIFFERENCE:
                val = str(e.constraint.value or "")
                painter.drawText(QRectF(mx - 14, my - 10, 28, 20),
                                 Qt.AlignmentFlag.AlignCenter, val)

    def _draw_vertices(self, painter: QPainter) -> None:
        for v in self.board.vertices():
            if v.watchtower is not None:
                x = self._padding + (v.col + 1) * self._cell_size
                y = self._padding + (v.row + 1) * self._cell_size
                r = self._cell_size // 5
                painter.setPen(Qt.PenStyle.NoPen)
                painter.setBrush(QBrush(QColor(_ui_theme.colors.watchtower_bg)))
                painter.drawEllipse(QPointF(x, y), r, r)
                painter.setPen(QPen(QColor(_ui_theme.colors.watchtower_border), 2))
                painter.drawEllipse(QPointF(x, y), r, r)
                font = QFont("Segoe UI", self._cell_size // 4, QFont.Weight.Bold)
                painter.setFont(font)
                painter.setPen(QPen(QColor(_ui_theme.colors.watchtower_text)))
                painter.drawText(QRectF(x - r, y - r, r * 2, r * 2),
                                 Qt.AlignmentFlag.AlignCenter, str(v.watchtower))

    def _draw_clues(self, painter: QPainter) -> None:
        for r in range(self.board.height):
            for c in range(self.board.width):
                cell = self.board.cell(r, c)
                rect = self._cell_rect(r, c)
                cx = rect.center().x()
                cy = rect.center().y()

                if cell.symbol is not None and cell.symbol:
                    font = QFont("Segoe UI", self._cell_size // 2, QFont.Weight.Bold)
                    painter.setFont(font)
                    painter.setPen(QPen(QColor(_ui_theme.colors.symbol_text)))
                    painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, cell.symbol)

                if cell.number is not None and cell.symbol is None:
                    font = QFont("Segoe UI", self._cell_size // 2, QFont.Weight.Bold)
                    painter.setFont(font)
                    painter.setPen(QPen(QColor(_ui_theme.colors.number_text)))
                    painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, str(cell.number))
                elif cell.number is not None:
                    font = QFont("Segoe UI", self._cell_size // 3, QFont.Weight.Bold)
                    painter.setFont(font)
                    painter.setPen(QPen(QColor(_ui_theme.colors.number_text)))
                    painter.drawText(QRectF(rect.x() + 4, rect.y() + 3,
                                              rect.width() - 8, rect.height() * 0.4),
                                     Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop, str(cell.number))

                if cell.compass is not None:
                    self._draw_compass(painter, cell, cx, cy)

                if cell.shape_pattern is not None:
                    self._draw_mini_shape_centered(
                        painter, cell.shape_pattern, cx, cy, self._cell_size * 0.6)

    def _draw_compass(self, painter: QPainter, cell: Cell, cx: float, cy: float) -> None:
        cp = cell.compass
        off = self._cell_size * 0.3
        font_small = QFont("Segoe UI", self._cell_size // 7)
        painter.setFont(font_small)
        painter.setPen(QPen(QColor(_ui_theme.colors.compass_text), 1))

        def draw_at(txt: str, dx: float, dy: float) -> None:
            if txt == "-1":
                txt = "∞"
            painter.drawText(QRectF(cx + dx - 12, cy + dy - 8, 24, 16),
                             Qt.AlignmentFlag.AlignCenter, txt)

        draw_at(str(cp.up) if cp.up >= 0 else "-1", 0, -off)
        draw_at(str(cp.down) if cp.down >= 0 else "-1", 0, off)
        draw_at(str(cp.left) if cp.left >= 0 else "-1", -off, 0)
        draw_at(str(cp.right) if cp.right >= 0 else "-1", off, 0)

        painter.setPen(QPen(QColor(_ui_theme.colors.compass_line), 1))
        painter.drawLine(QPointF(cx, cy), QPointF(cx, cy - off + 8))
        painter.drawLine(QPointF(cx, cy), QPointF(cx, cy + off - 8))
        painter.drawLine(QPointF(cx, cy), QPointF(cx - off + 8, cy))
        painter.drawLine(QPointF(cx, cy), QPointF(cx + off - 8, cy))

    def _draw_selection(self, painter: QPainter) -> None:
        if self._selected_cell is not None:
            r, c = self._selected_cell
            rect = self._cell_rect(r, c)
            painter.setPen(QPen(QColor(_ui_theme.colors.selection_border), 3))
            painter.drawRect(rect)
            if self._inline_number:
                font = QFont("Segoe UI", self._cell_size // 4, QFont.Weight.Bold)
                painter.setFont(font)
                painter.setPen(QPen(QColor(_ui_theme.colors.inline_number)))
                painter.drawText(QRectF(rect.x() + 4, rect.bottom() - rect.height() * 0.35,
                                          rect.width() * 0.6, rect.height() * 0.3),
                                 Qt.AlignmentFlag.AlignLeft, self._inline_number + "|")

        if self._selected_edge is not None:
            r1, c1, r2, c2 = self._selected_edge
            e = self.board.edge_between(r1, c1, r2, c2) if self.board is not None else None
            if e is not None:
                x1, y1, x2, y2 = self._edge_endpoints(e)
                painter.setPen(QPen(QColor(_ui_theme.colors.selection_border), 5))
                painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))

        if self._selected_vertex is not None or self._boundary_start_vertex is not None:
            v = self._selected_vertex if self._selected_vertex is not None else self._boundary_start_vertex
            if v is not None:
                x = self._padding + v[1] * self._cell_size
                y = self._padding + v[0] * self._cell_size
                painter.setPen(QPen(QColor(_ui_theme.colors.selection_border), 3))
                painter.setBrush(QBrush(QColor(_ui_theme.colors.selection_vertex_fill)))
                painter.drawEllipse(QPointF(x, y), self._cell_size // 6, self._cell_size // 6)

        if self._boundary_start_vertex is not None:
            x = self._padding + self._boundary_start_vertex[1] * self._cell_size
            y = self._padding + self._boundary_start_vertex[0] * self._cell_size
            painter.setPen(QPen(QColor(_ui_theme.colors.selection_border), 2, Qt.PenStyle.DashLine))
            painter.drawEllipse(QPointF(x, y), self._cell_size // 4, self._cell_size // 4)

        if self._hover_cell is not None and self._hover_cell != self._selected_cell:
            r, c = self._hover_cell
            rect = self._cell_rect(r, c)
            painter.setPen(QPen(QColor(_ui_theme.colors.hover_cell), 2))
            painter.drawRect(rect)

        if self._hover_vertex is not None and self._hover_vertex != self._selected_vertex:
            x = self._padding + self._hover_vertex[1] * self._cell_size
            y = self._padding + self._hover_vertex[0] * self._cell_size
            painter.setPen(QPen(QColor(_ui_theme.colors.hover_vertex), 2))
            painter.drawEllipse(QPointF(x, y), self._cell_size // 8, self._cell_size // 8)

        if self._hover_edge is not None and self._hover_edge != self._selected_edge:
            e = self.board.edge_between(*self._hover_edge)
            if e is not None:
                x1, y1, x2, y2 = self._edge_endpoints(e)
                painter.setPen(QPen(QColor(_ui_theme.colors.hover_cell), 3))
                painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))

    def _draw_rule_overlay(self, painter: QPainter) -> None:
        if not self._overlay_rules and not self._overlay_shapes:
            return
        grid_right = self._padding + self.board.width * self._cell_size
        grid_top = self._padding
        sz = 32
        pad = 8
        gap = 4
        font = QFont("Segoe UI", 10)
        painter.setFont(font)
        fm = painter.fontMetrics()

        # Build lines: rules first
        lines = list(self._overlay_rules)
        line_h = fm.height() + 4
        rules_h = len(lines) * line_h + gap

        # Shape pool section
        shapes_h = 0
        shape_rows: list[list[Shape]] = []
        if self._overlay_shapes:
            shapes_h += fm.height() + gap
            row: list[Shape] = []
            row_w = 0
            max_row_w = 160
            for s in self._overlay_shapes:
                rs2 = [r for r, _ in s.cells]
                cs2 = [c for _, c in s.cells]
                sw = (max(cs2) - min(cs2) + 2) * 14
                if row and row_w + sw > max_row_w:
                    shape_rows.append(row)
                    row = []
                    row_w = 0
                row.append(s)
                row_w += sw + 4
            if row:
                shape_rows.append(row)
            shapes_h += len(shape_rows) * (sz + gap)

        total_h = rules_h + shapes_h + pad * 2
        total_w = 180

        x0 = grid_right + pad
        y0 = grid_top + pad

        bg = QRectF(x0, y0, total_w, total_h)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QBrush(QColor(*_ui_theme.colors.overlay_bg)))
        painter.drawRoundedRect(bg, 8, 8)
        painter.setPen(QPen(QColor(_ui_theme.colors.overlay_border), 1))
        painter.drawRoundedRect(bg, 8, 8)

        cx = x0 + 8
        cy = y0 + pad

        painter.setPen(QPen(QColor(_ui_theme.colors.overlay_text)))
        for ln in lines:
            painter.drawText(QPointF(cx, cy + fm.ascent()), ln)
            cy += line_h

        if self._overlay_shapes:
            cy += gap
            painter.setPen(QPen(QColor(_ui_theme.colors.overlay_header)))
            painter.drawText(QPointF(cx, cy + fm.ascent()), "形状池")
            cy += fm.height() + gap
            painter.setPen(QPen(QColor(_ui_theme.colors.shape_mini_pen)))
            painter.setBrush(QBrush(QColor(_ui_theme.colors.shape_mini_fill)))
            for row in shape_rows:
                rx = cx
                for s in row:
                    rs2 = [r for r, _ in s.cells]
                    cs2 = [c for _, c in s.cells]
                    min_r2, max_r2 = min(rs2), max(rs2)
                    min_c2, max_c2 = min(cs2), max(cs2)
                    h2 = max_r2 - min_r2 + 1
                    w2 = max_c2 - min_c2 + 1
                    sc = min(12, (sz - 2) / max(h2, w2))
                    sh = sc * h2
                    sw2 = sc * w2
                    for r, c in s.cells:
                        nx = rx + (c - min_c2) * sc + (sw2 - w2 * sc) / 2
                        ny = cy + (r - min_r2) * sc
                        painter.drawRoundedRect(QRectF(nx, ny, sc - 0.5, sc - 0.5), 0.5, 0.5)
                    rx += sw2 + 6
                cy += sz + gap

    def _draw_mini_shape(self, painter: QPainter, shape: Shape, x0: float, y0: float, cell_sz: float) -> None:
        if not shape.cells:
            return
        rs = [r for r, _ in shape.cells]
        cs = [c for _, c in shape.cells]
        min_r, max_r = min(rs), max(rs)
        min_c, max_c = min(cs), max(cs)
        h = max_r - min_r + 1
        w = max_c - min_c + 1
        gap = 1
        scale = (cell_sz - gap * max(w, h)) / max(w, h) if max(w, h) > 0 else cell_sz
        scale = max(4, scale)
        painter.setPen(QPen(QColor(_ui_theme.colors.shape_mini_pen), 1))
        painter.setBrush(QBrush(QColor(_ui_theme.colors.shape_mini_fill)))
        for r, c in shape.cells:
            nx = x0 + (c - min_c) * (scale + gap)
            ny = y0 + (r - min_r) * (scale + gap)
            painter.drawRoundedRect(QRectF(nx, ny, scale, scale), 1, 1)

    def _draw_mini_shape_centered(self, painter: QPainter, shape: Shape,
                                   cx: float, cy: float, cell_sz: float) -> None:
        """Draw a mini shape scaled to cell_sz, centered on (cx, cy).

        Used to render puzzle-piece (shape_pattern) clues as a shape thumbnail
        inside the cell instead of a text label.
        """
        if not shape.cells:
            return
        rs = [r for r, _ in shape.cells]
        cs = [c for _, c in shape.cells]
        min_r, max_r = min(rs), max(rs)
        min_c, max_c = min(cs), max(cs)
        h = max_r - min_r + 1
        w = max_c - min_c + 1
        gap = 1
        scale = (cell_sz - gap * max(w, h)) / max(w, h) if max(w, h) > 0 else cell_sz
        scale = max(4, scale)
        total_w = (w - 1) * (scale + gap) + scale
        total_h = (h - 1) * (scale + gap) + scale
        x0 = cx - total_w / 2
        y0 = cy - total_h / 2
        painter.setPen(QPen(QColor(_ui_theme.colors.shape_mini_pen), 1))
        painter.setBrush(QBrush(QColor(_ui_theme.colors.shape_mini_fill)))
        for r, c in shape.cells:
            nx = x0 + (c - min_c) * (scale + gap)
            ny = y0 + (r - min_r) * (scale + gap)
            painter.drawRoundedRect(QRectF(nx, ny, scale, scale), 1, 1)
