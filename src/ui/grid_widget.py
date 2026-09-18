from __future__ import annotations

import re
from collections.abc import Callable
from typing import cast

from PySide6.QtCore import QPointF, QRectF, QSize, Qt, Signal
from PySide6.QtGui import (
    QBrush,
    QColor,
    QFont,
    QFontMetrics,
    QKeyEvent,
    QMouseEvent,
    QPainter,
    QPaintEvent,
    QPen,
    QWheelEvent,
)
from PySide6.QtWidgets import QMenu, QWidget

from src.models.board import (
    Board,
    Cell,
    CompassClue,
    Edge,
    EdgeConstraint,
    EdgeConstraintType,
    Shape,
)
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

# P-number coloured circles (matches the archive viewer)
P_COLORS = ["#c84030", "#3060c0", "#cca020", "#30a040", "#8030c0"]

# Fence diamond edges per F-value: (NW, NE, SW, SE) where NW = top-left corner,
# NE = top-right, SW = bottom-left, SE = bottom-right (matches the archive).
FENCE_EDGES: dict[str, tuple[int, int, int, int]] = {
    "F0": (0, 0, 0, 0),
    "F1": (0, 0, 0, 1),
    "F2": (0, 1, 1, 0),
    "F3": (0, 1, 1, 1),
    "F4": (1, 1, 1, 1),
    "F7": (1, 1, 0, 0),
}

# Arrow key -> (dr, dc) applied to the current selection.
ARROW_STEPS: dict[int, tuple[int, int]] = {
    Qt.Key.Key_Up: (-1, 0),
    Qt.Key.Key_Down: (1, 0),
    Qt.Key.Key_Left: (0, -1),
    Qt.Key.Key_Right: (0, 1),
}

# Keys that delete the content of the selected cell.
DELETE_KEYS = (Qt.Key.Key_Delete, Qt.Key.Key_Backspace)

# Keys that commit an in-progress inline number.
COMMIT_KEYS = (Qt.Key.Key_Return, Qt.Key.Key_Enter)


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

    def sizeHint(self) -> QSize:  # noqa: N802 — Qt override
        if self.board is None:
            return QSize(400, 300)
        w = self._padding * 2 + self.board.width * self._cell_size
        h = self._padding * 2 + self.board.height * self._cell_size
        return QSize(w, h)

    def minimumSizeHint(self) -> QSize:  # noqa: N802 — Qt override
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
        if not hasattr(self, "_cell_rects") or not self._cell_rects:
            return None
        for (r, c), rect in self._cell_rects.items():
            if rect.contains(pos):
                return (r, c)
        return None

    def _hit_test_vertex(self, pos: QPointF) -> tuple[int, int] | None:
        if self.board is None:
            return None
        if not hasattr(self, "_vertex_positions") or not self._vertex_positions:
            return None
        threshold = max(10, self._cell_size // 7)
        for (r, c), pt in self._vertex_positions.items():
            if abs(pos.x() - pt.x()) < threshold and abs(pos.y() - pt.y()) < threshold:
                return (r, c)
        return None

    def _hit_test_edge(self, pos: QPointF) -> tuple[int, int, int, int] | None:
        if self.board is None:
            return None
        if not hasattr(self, "_edge_rects") or not self._edge_rects:
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
            if dist < threshold and (best is None or dist < best[0]):
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

    def _outer_key(
        self, v1: tuple[int, int], v2: tuple[int, int]
    ) -> tuple[int, int, int, int] | None:
        r1, c1 = v1
        r2, c2 = v2
        if abs(r1 - r2) + abs(c1 - c2) != 1:
            return None
        board = cast("Board", self.board)
        if r1 == r2:
            c = min(c1, c2)
            if r1 == 0 or r1 == board.height:
                return (r1, c, r1, c + 1)
        if c1 == c2:
            r = min(r1, r2)
            if c1 == 0 or c1 == board.width:
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
        cell = cast("Board", self.board).cell(r, c)
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
        act_clear_all.setEnabled(
            not cell.blocked
            and (
                cell.number is not None
                or cell.symbol is not None
                or cell.compass is not None
                or cell.shape_pattern is not None
            )
        )
        act_clear_all.triggered.connect(lambda: self._clear_cell_properties(r, c))

        menu.addSeparator()

        act_toggle_boundary = menu.addAction("切换此格边框")
        act_toggle_boundary.triggered.connect(lambda: self._toggle_cell_boundary(r, c))

        menu.exec(self.mapToGlobal(pos.toPoint()))

    def _edge_context_menu(self, pos: QPointF, r1: int, c1: int, r2: int, c2: int) -> None:
        e = cast("Board", self.board).edge_between(r1, c1, r2, c2)
        if e is None:
            return
        menu = QMenu(self)

        act_toggle = menu.addAction("切换分割线")
        act_toggle.triggered.connect(lambda: self._toggle_edge_boundary(r1, c1, r2, c2))
        menu.addSeparator()

        act_hetero = menu.addAction("设异生 (≠)")
        act_hetero.setChecked(
            e.constraint is not None and e.constraint.type == EdgeConstraintType.HETEROGENEOUS
        )
        act_hetero.triggered.connect(
            lambda: self._set_edge_constraint(r1, c1, r2, c2, EdgeConstraintType.HETEROGENEOUS)
        )

        act_homo = menu.addAction("设双生 (=)")
        act_homo.setChecked(
            e.constraint is not None and e.constraint.type == EdgeConstraintType.HOMOGENEOUS
        )
        act_homo.triggered.connect(
            lambda: self._set_edge_constraint(r1, c1, r2, c2, EdgeConstraintType.HOMOGENEOUS)
        )

        act_ineq = menu.addAction("设不等号 (箭头)")
        act_ineq.triggered.connect(
            lambda: self._set_edge_constraint(r1, c1, r2, c2, EdgeConstraintType.INEQUALITY)
        )

        act_diff = menu.addAction("设差值")
        act_diff.triggered.connect(
            lambda: self._set_edge_constraint(r1, c1, r2, c2, EdgeConstraintType.DIFFERENCE, 1)
        )

        if e.constraint is not None:
            menu.addSeparator()
            act_clear = menu.addAction("清除约束")
            act_clear.triggered.connect(lambda: self._clear_edge_constraint(r1, c1, r2, c2))

        menu.exec(self.mapToGlobal(pos.toPoint()))

    def _vertex_context_menu(self, pos: QPointF, r: int, c: int) -> None:
        v = cast("Board", self.board).vertex_at(r, c)
        if v is None:
            return
        menu = QMenu(self)

        act_clear = menu.addAction("清除望塔值")
        act_clear.setEnabled(v.watchtower is not None)
        act_clear.triggered.connect(lambda: self._clear_watchtower(r, c))
        menu.exec(self.mapToGlobal(pos.toPoint()))

    def _paint_blocked(self, r: int, c: int, blocked: bool) -> None:
        cell = cast("Board", self.board).cell(r, c)
        if cell.blocked != blocked:
            cell.blocked = blocked
            if blocked:
                self._clear_cell(cell)
            self.board_modified.emit()
        self.update()

    def _toggle_blocked(self, r: int, c: int) -> None:
        cell = cast("Board", self.board).cell(r, c)
        cell.blocked = not cell.blocked
        if cell.blocked:
            self._clear_cell(cell)
        self._selected_cell = (r, c)
        self.board_modified.emit()
        self.update()

    def _set_cell_attr(self, r: int, c: int, attr: str, value: object) -> None:
        cell = cast("Board", self.board).cell(r, c)
        setattr(cell, attr, value)
        self.board_modified.emit()
        self.update()

    def _clear_cell_properties(self, r: int, c: int) -> None:
        cell = cast("Board", self.board).cell(r, c)
        if not cell.blocked:
            cell.number = None
            cell.symbol = None
            cell.compass = None
            cell.shape_pattern = None
            cell.fence_pattern = None
            cell.region_id = None
            self.board_modified.emit()
            self.update()

    def _toggle_cell_boundary(self, r: int, c: int) -> None:
        for e in cast("Board", self.board).edges():
            if (e.r1 == r and e.c1 == c) or (e.r2 == r and e.c2 == c):
                e.is_boundary = not e.is_boundary
        self.board_modified.emit()
        self.update()

    def _toggle_edge_boundary(self, r1: int, c1: int, r2: int, c2: int) -> None:
        e = cast("Board", self.board).edge_between(r1, c1, r2, c2)
        if e is not None:
            e.is_boundary = not e.is_boundary
            self.board_modified.emit()
            self.update()

    def _set_edge_constraint(
        self,
        r1: int,
        c1: int,
        r2: int,
        c2: int,
        ctype: EdgeConstraintType,
        value: int | None = None,
    ) -> None:
        e = cast("Board", self.board).edge_between(r1, c1, r2, c2)
        if e is not None:
            e.constraint = EdgeConstraint(type=ctype, value=value)
            self.board_modified.emit()
            self.edge_clicked.emit(r1, c1, r2, c2)
            self.update()

    def _clear_edge_constraint(self, r1: int, c1: int, r2: int, c2: int) -> None:
        e = cast("Board", self.board).edge_between(r1, c1, r2, c2)
        if e is not None:
            e.constraint = None
            self.board_modified.emit()
            self.edge_clicked.emit(r1, c1, r2, c2)
            self.update()

    def _clear_watchtower(self, r: int, c: int) -> None:
        v = cast("Board", self.board).vertex_at(r, c)
        if v is not None:
            v.watchtower = None
            self.board_modified.emit()
            self.update()

    def mousePressEvent(self, event: QMouseEvent) -> None:  # noqa: N802 — Qt override
        if self.board is None:
            return
        pos = event.position()

        vertex = self._hit_test_vertex(pos)
        edge = self._hit_test_edge(pos)
        cell = self._hit_test_cell(pos)

        if event.button() == Qt.MouseButton.RightButton:
            self._press_right_button(pos, vertex, edge, cell)
            return

        if event.button() != Qt.MouseButton.LeftButton:
            return

        handlers: dict[str, Callable[[], None]] = {
            self.MODE_BOUNDARY: lambda: self._press_boundary(pos, vertex, edge),
            self.MODE_WATCHTOWER: lambda: self._press_watchtower(vertex),
            self.MODE_BLOCK: lambda: self._press_block(cell),
            self.MODE_NUMBER: lambda: self._press_number(cell),
            self.MODE_SYMBOL: lambda: self._press_symbol(cell),
            self.MODE_COMPASS: lambda: self._press_compass(cell),
            self.MODE_SELECT: lambda: self._press_select(vertex, edge, cell),
        }
        handler = handlers.get(self._mode)
        if handler is not None:
            handler()

    def _press_right_button(
        self,
        pos: QPointF,
        vertex: tuple[int, int] | None,
        edge: tuple[int, int, int, int] | None,
        cell: tuple[int, int] | None,
    ) -> None:
        if self._mode == self.MODE_BLOCK:
            if cell is not None:
                self._block_dragging = True
                self._block_paint_blocked = False
                self._paint_blocked(cell[0], cell[1], False)
                self._selected_cell = cell
                self.update()
            return
        if vertex is not None:
            vr, vc = vertex[0], vertex[1]
            self._vertex_context_menu(pos, vr, vc)
        elif edge is not None:
            self._edge_context_menu(pos, *edge)
        elif cell is not None:
            self._cell_context_menu(pos, cell[0], cell[1])

    def _press_boundary(
        self, _pos: QPointF, vertex: tuple[int, int] | None, edge: tuple[int, int, int, int] | None
    ) -> None:
        if vertex is not None:
            if edge is not None:
                self._toggle_edge_boundary(*edge)
                self._selected_edge = edge
                self._selected_vertex = None
                self._selected_cell = None
                self.update()
                return
            self._boundary_dragging = True
            self._last_boundary_vertex = vertex
            self._selected_vertex = vertex
            self._boundary_start_vertex = None
            self.update()
        elif edge is not None:
            self._toggle_edge_boundary(*edge)
            self._selected_edge = edge
            self._selected_vertex = None
            self._selected_cell = None
            self.update()

    def _press_watchtower(self, vertex: tuple[int, int] | None) -> None:
        if vertex is not None:
            vr, vc = vertex[0], vertex[1]
            v = cast("Board", self.board).vertex_at(vr, vc)
            if v is not None:
                val = self._current_number
                if val is not None and 1 <= val <= 4:
                    v.watchtower = val
                self._selected_vertex = vertex
                self._selected_cell = None
                self._selected_edge = None
                self.board_modified.emit()
                self.vertex_clicked.emit(vr, vc)
                self.update()

    def _press_block(self, cell: tuple[int, int] | None) -> None:
        if cell is not None:
            self._block_dragging = True
            self._block_paint_blocked = True
            self._paint_blocked(cell[0], cell[1], True)
            self._selected_cell = cell
            self.update()

    def _press_number(self, cell: tuple[int, int] | None) -> None:
        if cell is not None:
            c_obj = cast("Board", self.board).cell(cell[0], cell[1])
            if self._current_number is not None:
                c_obj.number = self._current_number
            self._selected_cell = cell
            self._selected_edge = None
            self._selected_vertex = None
            self._inline_number = ""
            self.board_modified.emit()
            self.setFocus()
            self.update()
            self.cell_clicked.emit(cell[0], cell[1])

    def _press_symbol(self, cell: tuple[int, int] | None) -> None:
        if cell is not None:
            c_obj = cast("Board", self.board).cell(cell[0], cell[1])
            c_obj.symbol = self._current_symbol
            self._selected_cell = cell
            self._selected_edge = None
            self._selected_vertex = None
            self.board_modified.emit()
            self.update()
            self.cell_clicked.emit(cell[0], cell[1])

    def _press_compass(self, cell: tuple[int, int] | None) -> None:
        if cell is not None:
            c_obj = cast("Board", self.board).cell(cell[0], cell[1])
            c_obj.compass = self._current_compass
            self._selected_cell = cell
            self._selected_edge = None
            self._selected_vertex = None
            self.board_modified.emit()
            self.update()
            self.cell_clicked.emit(cell[0], cell[1])

    def _press_select(
        self,
        vertex: tuple[int, int] | None,
        edge: tuple[int, int, int, int] | None,
        cell: tuple[int, int] | None,
    ) -> None:
        if vertex is not None:
            self._selected_vertex = vertex
            self._selected_cell = None
            self._selected_edge = None
            self.vertex_clicked.emit(vertex[0], vertex[1])
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

    def _toggle_boundary_segment(self, start: tuple[int, int], vertex: tuple[int, int]) -> None:
        """Toggle the boundary between two adjacent vertices (inner edge, or
        an outer-boundary edge). Emits ``edge_clicked`` for inner edges; the
        caller is responsible for ``board_modified``/``update``."""
        e = self._vertices_to_edge(start, vertex)
        if e is not None:
            e.is_boundary = not e.is_boundary
            self.edge_clicked.emit(e.r1, e.c1, e.r2, e.c2)
            return
        okey = self._outer_key(start, vertex)
        if okey is not None:
            if okey in self._outer_boundaries:
                self._outer_boundaries.discard(okey)
                if self.board is not None:
                    self.board.outer_boundaries = [
                        k for k in self.board.outer_boundaries if k != okey
                    ]
            else:
                self._outer_boundaries.add(okey)
                if self.board is not None:
                    self.board.outer_boundaries.append(okey)

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
            self._toggle_boundary_segment(start, vertex)

        self._boundary_start_vertex = None
        self._selected_vertex = None
        self.board_modified.emit()
        self.update()

    def mouseMoveEvent(self, event: QMouseEvent) -> None:  # noqa: N802 — Qt override
        if self.board is None:
            return
        pos = event.position()

        if self._mode == self.MODE_BLOCK and self._block_dragging:
            self._drag_block_mouse(pos)
            return

        if self._mode == self.MODE_WATCHTOWER:
            self._hover_vertex_mode_mouse(pos)
            return

        if self._mode == self.MODE_BOUNDARY:
            if self._boundary_dragging and self._last_boundary_vertex is not None:
                self._drag_boundary_mouse(pos)
            else:
                self._hover_vertex_mode_mouse(pos)
            return

        self._hover_select_mode_mouse(pos)

    def _drag_block_mouse(self, pos: QPointF) -> None:
        cell = self._hit_test_cell(pos)
        if cell is not None:
            self._paint_blocked(cell[0], cell[1], self._block_paint_blocked)
            self._hover_cell = cell
            self.status_message.emit(f"障碍模式: 单元格 ({cell[0]}, {cell[1]})")

    def _drag_boundary_mouse(self, pos: QPointF) -> None:
        v = self._hit_test_vertex(pos)
        last = self._last_boundary_vertex
        if v is not None and last is not None and v != last and self._vertices_adjacent(last, v):
            self._toggle_boundary_segment(last, v)
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

    def _hover_vertex_mode_mouse(self, pos: QPointF) -> None:
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

    def _hover_select_mode_mouse(self, pos: QPointF) -> None:
        v = self._hit_test_vertex(pos)
        e = self._hit_test_edge(pos)
        changed = False
        if v is not None:
            changed = self._set_hover_vertex(v)
            self.status_message.emit(f"顶点 ({v[0]}, {v[1]})")
        elif e is not None:
            changed = self._set_hover_edge(e)
            self.status_message.emit(f"边框 ({e[0]},{e[1]})-({e[2]},{e[3]})")
        else:
            changed = self._set_hover_cell(pos)
        if changed:
            self.update()

    def _set_hover_vertex(self, v: tuple[int, int]) -> bool:
        if v != self._hover_vertex:
            self._hover_vertex = v
            self._hover_cell = None
            self._hover_edge = None
            return True
        return False

    def _set_hover_edge(self, e: tuple[int, int, int, int]) -> bool:
        if e != self._hover_edge:
            self._hover_edge = e
            self._hover_cell = None
            self._hover_vertex = None
            return True
        return False

    def _set_hover_cell(self, pos: QPointF) -> bool:
        cell = self._hit_test_cell(pos)
        changed = False
        if self._hover_edge is not None or self._hover_vertex is not None:
            changed = True
        self._hover_edge = None
        self._hover_vertex = None
        if cell != self._hover_cell:
            self._hover_cell = cell
            changed = True
        if cell is not None:
            self._emit_cell_status(cell)
        elif not self._hover_vertex:
            self.status_message.emit("")
        return changed

    def _emit_cell_status(self, cell: tuple[int, int]) -> None:
        c_obj = cast("Board", self.board).cell(cell[0], cell[1])
        extras = []
        if c_obj.number is not None:
            extras.append(f"#{c_obj.number}")
        if c_obj.symbol is not None:
            extras.append(f"符号:{c_obj.symbol}")
        if c_obj.blocked:
            extras.append("障碍")
        suffix = f" [{' '.join(extras)}]" if extras else ""
        self.status_message.emit(f"单元格 ({cell[0]}, {cell[1]}){suffix}")

    def keyPressEvent(self, event: QKeyEvent) -> None:  # noqa: N802 — Qt override
        if self.board is None:
            return
        if self._handle_key(event):
            return
        step = ARROW_STEPS.get(event.key())
        if step is None:
            super().keyPressEvent(event)
        else:
            self._move_selection(*step)

    def _handle_key(self, event: QKeyEvent) -> bool:
        """Handle a key press; return True when the event was consumed."""
        key = event.key()
        text = event.text()
        if key == Qt.Key.Key_Escape:
            self._clear_selection_state()
            return True
        if key in DELETE_KEYS:
            return self._clear_selected_cell()
        if self._selected_cell is None:
            return False
        if self._mode == self.MODE_NUMBER and self._handle_number_key(key, text):
            return True
        if text.isdigit():
            self._assign_digit(int(text))
            return True
        return False

    def _clear_selection_state(self) -> None:
        self._selected_cell = None
        self._selected_edge = None
        self._selected_vertex = None
        self._inline_number = ""
        self.update()

    def _clear_selected_cell(self) -> bool:
        if self._selected_cell is None:
            return False
        r, c = self._selected_cell
        cell = cast("Board", self.board).cell(r, c)
        if not cell.blocked:
            cell.number = None
            cell.symbol = None
            cell.compass = None
            self._inline_number = ""
            self.board_modified.emit()
            self.update()
        return True

    def _handle_number_key(self, key: int, text: str) -> bool:
        """Number-mode editing: digits extend the inline buffer, Enter commits."""
        if text.isdigit():
            self._inline_number += text
            self._write_selected_number(int(self._inline_number))
            self.board_modified.emit()
            self.update()
            return True
        if key in COMMIT_KEYS:
            if self._inline_number:
                self._write_selected_number(int(self._inline_number))
                self._inline_number = ""
                self.board_modified.emit()
                self._move_selection(0, 1)
                self.update()
            return True
        return False

    def _write_selected_number(self, value: int) -> None:
        selected = self._selected_cell
        if selected is None:
            return
        r, c = selected
        cast("Board", self.board).cell(r, c).number = value

    def _assign_digit(self, value: int) -> None:
        selected = self._selected_cell
        if selected is None:
            return
        r, c = selected
        cell = cast("Board", self.board).cell(r, c)
        if cell.blocked:
            return
        cell.number = value
        self._inline_number = ""
        self.board_modified.emit()
        self.update()

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

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:  # noqa: N802 — Qt override
        if self._block_dragging:
            self._block_dragging = False
        if event.button() == Qt.MouseButton.LeftButton and self._boundary_dragging:
            self._boundary_dragging = False
            self._last_boundary_vertex = None
            self.update()

    def wheelEvent(self, event: QWheelEvent) -> None:  # noqa: N802 — Qt override
        if event.angleDelta().y() > 0:
            self._cell_size = min(120, self._cell_size + 5)
        else:
            self._cell_size = max(15, self._cell_size - 5)
        self._cache_rects()
        self.update()

    def paintEvent(self, _event: QPaintEvent) -> None:  # noqa: N802 — Qt override
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
        board = cast("Board", self.board)
        c1 = board.cell(e.r1, e.c1)
        c2 = board.cell(e.r2, e.c2)
        return (
            not c1.blocked
            and not c2.blocked
            and c1.region_id is not None
            and c1.region_id == c2.region_id
        )

    def _draw_cells(self, painter: QPainter) -> None:
        board = cast("Board", self.board)
        for r in range(board.height):
            for c in range(board.width):
                cell = board.cell(r, c)
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
        board = cast("Board", self.board)
        c1 = board.cell(e.r1, e.c1)
        c2 = board.cell(e.r2, e.c2)
        return c1.blocked != c2.blocked

    def _draw_boundary_edges(self, painter: QPainter) -> None:
        board = cast("Board", self.board)
        pen = QPen(QColor(_ui_theme.colors.boundary_edge), 6)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        painter.setPen(pen)
        for e in board.edges():
            if e.is_boundary or self._is_auto_boundary(e):
                x1, y1, x2, y2 = self._edge_endpoints(e)
                painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))
        for key in self._outer_boundaries:
            self._draw_outer_edge(painter, key)

        painter.setPen(QPen(QColor(_ui_theme.colors.boundary_highlight), 2.5))
        for e in board.edges():
            if e.is_boundary or self._is_auto_boundary(e):
                x1, y1, x2, y2 = self._edge_endpoints(e)
                painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))
        for key in self._outer_boundaries:
            self._draw_outer_edge(painter, key)

    def _draw_grid_lines(self, painter: QPainter) -> None:
        board = cast("Board", self.board)
        painter.setPen(QPen(QColor(_ui_theme.colors.grid_line), 1))
        for e in board.edges():
            if self._is_same_region_internal(e):
                continue
            x1, y1, x2, y2 = self._edge_endpoints(e)
            painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))
        # outer perimeter so the board outline stays visible even when the
        # puzzle carries no explicit outer_boundaries
        pad, sz = self._padding, self._cell_size
        board = cast("Board", self.board)
        painter.drawRect(QRectF(pad, pad, board.width * sz, board.height * sz))

    def _draw_edge_constraints(self, painter: QPainter) -> None:
        for e in cast("Board", self.board).edges():
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
            handler: Callable[[], None] | None = {
                EdgeConstraintType.HETEROGENEOUS: lambda mx=mx, my=my, sz=sz: (
                    self._draw_edge_heterogeneous(painter, mx, my, sz)
                ),
                EdgeConstraintType.HOMOGENEOUS: lambda mx=mx, my=my, sz=sz: (
                    self._draw_edge_homogeneous(painter, mx, my, sz)
                ),
                EdgeConstraintType.INEQUALITY: lambda e=e, mx=mx, my=my: self._draw_edge_inequality(
                    painter, e, mx, my
                ),
                EdgeConstraintType.DIFFERENCE: lambda e=e, mx=mx, my=my: self._draw_edge_difference(
                    painter, e, mx, my
                ),
            }.get(ct)
            if handler is not None:
                handler()

    def _draw_edge_heterogeneous(self, painter: QPainter, mx: float, my: float, sz: float) -> None:
        # diff (异生): black box with δ, matches the archive viewer
        bx = QRectF(mx - sz * 0.55, my - sz * 0.55, sz * 1.1, sz * 1.1)
        painter.setPen(QPen(QColor("#555"), 1))
        painter.setBrush(QBrush(QColor("#111")))
        painter.drawRect(bx)
        painter.setPen(QPen(QColor("#ffffff"), 1))
        painter.setFont(QFont("Segoe UI Symbol", int(sz * 0.7), QFont.Weight.Bold))
        painter.drawText(bx, Qt.AlignmentFlag.AlignCenter, "δ")
        painter.setBrush(Qt.BrushStyle.NoBrush)

    def _draw_edge_homogeneous(self, painter: QPainter, mx: float, my: float, sz: float) -> None:
        # twins (双生): white box with ♂, matches the archive viewer
        bx = QRectF(mx - sz * 0.55, my - sz * 0.55, sz * 1.1, sz * 1.1)
        painter.setPen(QPen(QColor("#555"), 1))
        painter.setBrush(QBrush(QColor("#ffffff")))
        painter.drawRect(bx)
        painter.setPen(QPen(QColor("#111"), 1))
        painter.setFont(QFont("Segoe UI Symbol", int(sz * 0.7), QFont.Weight.Bold))
        painter.drawText(bx, Qt.AlignmentFlag.AlignCenter, "♂")
        painter.setBrush(Qt.BrushStyle.NoBrush)

    def _draw_edge_inequality(self, painter: QPainter, e: Edge, mx: float, my: float) -> None:
        # 不等号：value==1 → 第一端点 (r1,c1) 更大；否则第二端点更大。
        # 用数学不等号直接表达“哪一侧更大”：
        #   '>' 左侧更大 · '<' 右侧更大 · '^' 上方更大 · 'v' 下方更大
        # （符号尖始终指向面积更小的一侧，与游戏规则一致）
        rev = e.constraint is not None and e.constraint.value == 1
        if e.c1 == e.c2:
            # 垂直边：更大侧在 r1 或 r2 端
            sym = ("^" if e.r1 < e.r2 else "v") if rev else ("v" if e.r1 < e.r2 else "^")
        else:
            # 水平边：更大侧在 c1 或 c2 端
            sym = (">" if e.c1 < e.c2 else "<") if rev else ("<" if e.c1 < e.c2 else ">")
        painter.drawText(QRectF(mx - 14, my - 10, 28, 20), Qt.AlignmentFlag.AlignCenter, sym)

    def _draw_edge_difference(self, painter: QPainter, e: Edge, mx: float, my: float) -> None:
        val = "" if e.constraint is None else str(e.constraint.value or "")
        painter.drawText(QRectF(mx - 14, my - 10, 28, 20), Qt.AlignmentFlag.AlignCenter, val)

    def _draw_vertices(self, painter: QPainter) -> None:
        for v in cast("Board", self.board).vertices():
            if v.watchtower is not None:
                # Vertex (r,c) is the ABSOLUTE grid corner (0..=h × 0..=w).
                x = self._padding + v.col * self._cell_size
                y = self._padding + v.row * self._cell_size
                r = self._cell_size / 5
                painter.setPen(Qt.PenStyle.NoPen)
                painter.setBrush(QBrush(QColor(_ui_theme.colors.watchtower_bg)))
                painter.drawEllipse(QPointF(x, y), r, r)
                painter.setPen(QPen(QColor(_ui_theme.colors.watchtower_border), 2))
                painter.drawEllipse(QPointF(x, y), r, r)
                # dice dots (matches the archive viewer), falls back to the
                # number for out-of-range values
                dots_map = {
                    1: [(0.0, 0.0)],
                    2: [(-0.4, 0.4), (0.4, -0.4)],
                    3: [(-0.4, 0.4), (0.0, 0.0), (0.4, -0.4)],
                    4: [(-0.4, -0.4), (0.4, -0.4), (-0.4, 0.4), (0.4, 0.4)],
                }
                dots = dots_map.get(v.watchtower)
                if dots is not None:
                    painter.setPen(Qt.PenStyle.NoPen)
                    painter.setBrush(QBrush(QColor(_ui_theme.colors.watchtower_text)))
                    for dx, dy in dots:
                        painter.drawEllipse(QPointF(x + dx * r, y + dy * r), r * 0.25, r * 0.25)
                    painter.setBrush(Qt.BrushStyle.NoBrush)
                else:
                    font = QFont("Segoe UI", int(self._cell_size // 4), QFont.Weight.Bold)
                    painter.setFont(font)
                    painter.setPen(QPen(QColor(_ui_theme.colors.watchtower_text)))
                    painter.drawText(
                        QRectF(x - r, y - r, r * 2, r * 2),
                        Qt.AlignmentFlag.AlignCenter,
                        str(v.watchtower),
                    )

    def _draw_clues(self, painter: QPainter) -> None:
        board = cast("Board", self.board)
        for r in range(board.height):
            for c in range(board.width):
                cell = board.cell(r, c)
                rect = self._cell_rect(r, c)
                cx = rect.center().x()
                cy = rect.center().y()

                self._draw_cell_symbol(painter, cell, rect, cx, cy)
                if cell.fence_pattern is not None:
                    self._draw_fence_diamond(
                        painter, cx, cy, self._fence_diamond(cell.fence_pattern)
                    )
                self._draw_cell_number(painter, cell, rect, cx, cy)
                if cell.compass is not None:
                    self._draw_compass(painter, cell, cx, cy)
                if cell.shape_pattern is not None:
                    self._draw_mini_shape_centered(
                        painter, cell.shape_pattern, cx, cy, self._cell_size * 0.6
                    )

    def _draw_cell_symbol(
        self, painter: QPainter, cell: Cell, rect: QRectF, cx: float, cy: float
    ) -> None:
        if cell.symbol is not None and cell.symbol:
            if re.fullmatch(r"P[1-9]", cell.symbol):
                self._draw_p_circle(painter, cx, cy, int(cell.symbol[1]))
            else:
                font = QFont("Segoe UI", self._cell_size // 2, QFont.Weight.Bold)
                painter.setFont(font)
                painter.setPen(QPen(QColor(_ui_theme.colors.symbol_text)))
                painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, cell.symbol)

    def _draw_cell_number(
        self, painter: QPainter, cell: Cell, rect: QRectF, _cx: float, _cy: float
    ) -> None:
        if cell.number is not None and cell.symbol is None:
            font = QFont("Segoe UI", self._cell_size // 2, QFont.Weight.Bold)
            painter.setFont(font)
            painter.setPen(QPen(QColor(_ui_theme.colors.number_text)))
            painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, str(cell.number))
        elif cell.number is not None:
            font = QFont("Segoe UI", self._cell_size // 3, QFont.Weight.Bold)
            painter.setFont(font)
            painter.setPen(QPen(QColor(_ui_theme.colors.number_text)))
            painter.drawText(
                QRectF(rect.x() + 4, rect.y() + 3, rect.width() - 8, rect.height() * 0.4),
                Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop,
                str(cell.number),
            )

    def _draw_p_circle(self, painter: QPainter, cx: float, cy: float, n: int) -> None:
        """P-number coloured circle (matches the archive viewer)."""
        r = self._cell_size * 0.24
        color = P_COLORS[(n - 1) % len(P_COLORS)]
        painter.setPen(QPen(QColor("#000000"), 1.5))
        painter.setBrush(QBrush(QColor(color)))
        painter.drawEllipse(QPointF(cx, cy), r, r)
        painter.setBrush(Qt.BrushStyle.NoBrush)

    def _fence_diamond(self, pattern: Shape) -> str:
        """Recover the F-value from the stored 3x3 directional pattern.

        The pattern is {center} plus up/down/left/right boundary bits.  The
        diamond edge set per F-value matches the archive viewer.
        """
        cells = pattern.cells
        bits = {
            "up": (0, 1) in cells,
            "down": (2, 1) in cells,
            "left": (1, 0) in cells,
            "right": (1, 2) in cells,
        }
        count = sum(bits.values())
        if count == 0:
            return "F0"
        if count == 1:
            return "F1"
        if count == 2:
            if (bits["up"] and bits["down"]) or (bits["left"] and bits["right"]):
                return "F2"
            return "F7"
        if count == 3:
            return "F3"
        return "F4"

    def _draw_fence_diamond(self, painter: QPainter, cx: float, cy: float, fval: str) -> None:
        """Fence diamond: solid for present edges, dotted for absent (matches
        the archive viewer's renderFence)."""
        nw, ne, sw, se = FENCE_EDGES[fval]
        r = self._cell_size * 0.35
        t = QPointF(cx, cy - r)
        rp = QPointF(cx + r, cy)
        b = QPointF(cx, cy + r)
        lft = QPointF(cx - r, cy)
        segments = [(nw, t, lft), (ne, t, rp), (sw, lft, b), (se, rp, b)]

        painter.setPen(QPen(QColor(80, 60, 30, 70), 1, Qt.PenStyle.DashLine))
        for present, p1, p2 in segments:
            if not present:
                painter.drawLine(p1, p2)
        painter.setPen(QPen(QColor("#2a1a08"), 1.8))
        for present, p1, p2 in segments:
            if present:
                painter.drawLine(p1, p2)
        painter.setBrush(QBrush(QColor("#2a1a08")))
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawEllipse(QPointF(cx, cy), self._cell_size * 0.09, self._cell_size * 0.09)
        painter.setBrush(Qt.BrushStyle.NoBrush)

    def _draw_compass(self, painter: QPainter, cell: Cell, cx: float, cy: float) -> None:
        cp = cell.compass
        if cp is None:
            return
        off = self._cell_size * 0.3
        font_small = QFont("Segoe UI", self._cell_size // 7)
        painter.setFont(font_small)
        painter.setPen(QPen(QColor(_ui_theme.colors.compass_text), 1))

        def draw_at(value: int, dx: float, dy: float) -> None:
            if value < 0:
                return  # unconstrained direction: not drawn (matches the archive)
            painter.drawText(
                QRectF(cx + dx - 12, cy + dy - 8, 24, 16), Qt.AlignmentFlag.AlignCenter, str(value)
            )

        draw_at(cp.up, 0, -off)
        draw_at(cp.down, 0, off)
        draw_at(cp.left, -off, 0)
        draw_at(cp.right, off, 0)

        painter.setPen(QPen(QColor(_ui_theme.colors.compass_line), 1))
        painter.drawLine(QPointF(cx, cy), QPointF(cx, cy - off + 8))
        painter.drawLine(QPointF(cx, cy), QPointF(cx, cy + off - 8))
        painter.drawLine(QPointF(cx, cy), QPointF(cx - off + 8, cy))
        painter.drawLine(QPointF(cx, cy), QPointF(cx + off - 8, cy))

    def _draw_selection(self, painter: QPainter) -> None:
        self._draw_selected_cell(painter)
        self._draw_selected_edge(painter)
        self._draw_selected_vertex(painter)
        self._draw_boundary_start_marker(painter)
        self._draw_hover_cell(painter)
        self._draw_hover_vertex(painter)
        self._draw_hover_edge(painter)

    def _draw_selected_cell(self, painter: QPainter) -> None:
        if self._selected_cell is None:
            return
        r, c = self._selected_cell
        rect = self._cell_rect(r, c)
        painter.setPen(QPen(QColor(_ui_theme.colors.selection_border), 3))
        painter.drawRect(rect)
        if not self._inline_number:
            return
        font = QFont("Segoe UI", self._cell_size // 4, QFont.Weight.Bold)
        painter.setFont(font)
        painter.setPen(QPen(QColor(_ui_theme.colors.inline_number)))
        painter.drawText(
            QRectF(
                rect.x() + 4,
                rect.bottom() - rect.height() * 0.35,
                rect.width() * 0.6,
                rect.height() * 0.3,
            ),
            Qt.AlignmentFlag.AlignLeft,
            self._inline_number + "|",
        )

    def _draw_selected_edge(self, painter: QPainter) -> None:
        if self._selected_edge is None:
            return
        r1, c1, r2, c2 = self._selected_edge
        e = self.board.edge_between(r1, c1, r2, c2) if self.board is not None else None
        if e is None:
            return
        x1, y1, x2, y2 = self._edge_endpoints(e)
        painter.setPen(QPen(QColor(_ui_theme.colors.selection_border), 5))
        painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))

    def _draw_selected_vertex(self, painter: QPainter) -> None:
        selected = self._selected_vertex
        v = selected if selected is not None else self._boundary_start_vertex
        if v is None:
            return
        x = self._padding + v[1] * self._cell_size
        y = self._padding + v[0] * self._cell_size
        painter.setPen(QPen(QColor(_ui_theme.colors.selection_border), 3))
        painter.setBrush(QBrush(QColor(_ui_theme.colors.selection_vertex_fill)))
        painter.drawEllipse(QPointF(x, y), self._cell_size // 6, self._cell_size // 6)

    def _draw_boundary_start_marker(self, painter: QPainter) -> None:
        if self._boundary_start_vertex is None:
            return
        x = self._padding + self._boundary_start_vertex[1] * self._cell_size
        y = self._padding + self._boundary_start_vertex[0] * self._cell_size
        painter.setPen(QPen(QColor(_ui_theme.colors.selection_border), 2, Qt.PenStyle.DashLine))
        painter.drawEllipse(QPointF(x, y), self._cell_size // 4, self._cell_size // 4)

    def _draw_hover_cell(self, painter: QPainter) -> None:
        if self._hover_cell is None or self._hover_cell == self._selected_cell:
            return
        r, c = self._hover_cell
        rect = self._cell_rect(r, c)
        painter.setPen(QPen(QColor(_ui_theme.colors.hover_cell), 2))
        painter.drawRect(rect)

    def _vertex_center(self, v: tuple[int, int]) -> QPointF:
        return QPointF(
            self._padding + v[1] * self._cell_size, self._padding + v[0] * self._cell_size
        )

    def _draw_hover_vertex(self, painter: QPainter) -> None:
        if self._hover_vertex is None or self._hover_vertex == self._selected_vertex:
            return
        painter.setPen(QPen(QColor(_ui_theme.colors.hover_vertex), 2))
        painter.drawEllipse(
            self._vertex_center(self._hover_vertex), self._cell_size // 8, self._cell_size // 8
        )

    def _draw_hover_edge(self, painter: QPainter) -> None:
        if self._hover_edge is None or self._hover_edge == self._selected_edge:
            return
        e = cast("Board", self.board).edge_between(*self._hover_edge)
        if e is None:
            return
        x1, y1, x2, y2 = self._edge_endpoints(e)
        painter.setPen(QPen(QColor(_ui_theme.colors.hover_cell), 3))
        painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))

    def _draw_rule_overlay(self, painter: QPainter) -> None:
        if not self._overlay_rules and not self._overlay_shapes:
            return
        grid_right = self._padding + cast("Board", self.board).width * self._cell_size
        grid_top = self._padding
        sz = 32
        pad = 8
        gap = 4
        font = QFont("Segoe UI", 10)
        painter.setFont(font)
        fm = painter.fontMetrics()

        line_h = fm.height() + 4
        rules_h = len(self._overlay_rules) * line_h + gap
        shape_rows = self._pack_shape_rows() if self._overlay_shapes else []
        shapes_h = 0
        if self._overlay_shapes:
            shapes_h = fm.height() + gap + len(shape_rows) * (sz + gap)

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
        for ln in self._overlay_rules:
            painter.drawText(QPointF(cx, cy + fm.ascent()), ln)
            cy += line_h

        if self._overlay_shapes:
            self._draw_overlay_shapes(painter, shape_rows, cx, cy + gap, fm, sz, gap)

    def _pack_shape_rows(self, max_row_w: int = 160) -> list[list[Shape]]:
        """Group the overlay shapes into rows whose total width fits max_row_w."""
        rows: list[list[Shape]] = []
        row: list[Shape] = []
        row_w = 0
        for s in self._overlay_shapes:
            cs = [c for _, c in s.cells]
            sw = (max(cs) - min(cs) + 2) * 14
            if row and row_w + sw > max_row_w:
                rows.append(row)
                row = []
                row_w = 0
            row.append(s)
            row_w += sw + 4
        if row:
            rows.append(row)
        return rows

    def _draw_overlay_shapes(
        self,
        painter: QPainter,
        shape_rows: list[list[Shape]],
        cx: float,
        cy: float,
        fm: QFontMetrics,
        sz: float,
        gap: int,
    ) -> None:
        painter.setPen(QPen(QColor(_ui_theme.colors.overlay_header)))
        painter.drawText(QPointF(cx, cy + fm.ascent()), "形状池")
        cy += fm.height() + gap
        painter.setPen(QPen(QColor(_ui_theme.colors.shape_mini_pen)))
        painter.setBrush(QBrush(QColor(_ui_theme.colors.shape_mini_fill)))
        for row in shape_rows:
            rx = cx
            for s in row:
                rx = self._draw_overlay_shape(painter, s, rx, cy, sz)
            cy += sz + gap

    def _draw_overlay_shape(
        self, painter: QPainter, s: Shape, rx: float, cy: float, sz: float
    ) -> float:
        """Draw one mini shape at (rx, cy); return the next row x offset."""
        rs = [r for r, _ in s.cells]
        cs = [c for _, c in s.cells]
        min_r, max_r = min(rs), max(rs)
        min_c, max_c = min(cs), max(cs)
        h = max_r - min_r + 1
        w = max_c - min_c + 1
        sc = min(12, (sz - 2) / max(h, w))
        sw = sc * w
        for r, c in s.cells:
            nx = rx + (c - min_c) * sc + (sw - w * sc) / 2
            ny = cy + (r - min_r) * sc
            painter.drawRoundedRect(QRectF(nx, ny, sc - 0.5, sc - 0.5), 0.5, 0.5)
        return rx + sw + 6

    def _draw_mini_shape(
        self, painter: QPainter, shape: Shape, x0: float, y0: float, cell_sz: float
    ) -> None:
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

    def _draw_mini_shape_centered(
        self, painter: QPainter, shape: Shape, cx: float, cy: float, cell_sz: float
    ) -> None:
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
