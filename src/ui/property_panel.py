from __future__ import annotations

from typing import Optional

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QGridLayout,
    QLabel, QPushButton, QSpinBox, QLineEdit, QCheckBox,
    QFrame, QScrollArea,
)

from src.models.board import Board, EdgeConstraint, EdgeConstraintType


SPIN_STYLE = """
QSpinBox { min-width: 80px; }
"""


def _sep(layout: QVBoxLayout) -> None:
    line = QFrame()
    line.setFrameShape(QFrame.Shape.HLine)
    line.setStyleSheet("background: palette(mid); max-height: 1px; margin: 2px 0;")
    layout.addWidget(line)


class PropertyPanel(QWidget):
    board_modified = Signal()

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._board: Board | None = None
        self._selected_cell: tuple[int, int] | None = None
        self._selected_edge: tuple[int, int, int, int] | None = None
        self._selected_vertex: tuple[int, int] | None = None
        self._setup_ui()

    def _setup_ui(self) -> None:
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setStyleSheet("QScrollArea { border: none; background: transparent; }")

        container = QWidget()
        self._layout = QVBoxLayout(container)
        self._layout.setContentsMargins(8, 8, 8, 8)
        self._layout.setSpacing(6)

        title = QLabel("属性面板")
        title.setStyleSheet("font-size: 14px; font-weight: bold;")
        self._layout.addWidget(title)
        _sep(self._layout)

        self._info = QLabel("未选中任何对象")
        self._info.setWordWrap(True)
        self._info.setStyleSheet(
            "font-size: 12px; padding: 8px; "
            "border: 1px solid palette(mid); border-radius: 6px;"
        )
        self._layout.addWidget(self._info)

        self._content = QVBoxLayout()
        self._content.setSpacing(4)
        self._layout.addLayout(self._content)
        self._layout.addStretch()

        scroll.setWidget(container)
        outer.addWidget(scroll)

    def set_board(self, board: Board) -> None:
        self._board = board
        self.clear_selection()

    def clear_selection(self) -> None:
        self._selected_cell = None
        self._selected_edge = None
        self._selected_vertex = None
        self._info.setText("未选中任何对象")
        self._clear_content()

    def _clear_content(self) -> None:
        while self._content.count():
            item = self._content.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
            elif item.layout():
                while item.layout().count():
                    child = item.layout().takeAt(0)
                    if child.widget():
                        child.widget().deleteLater()

    def select_cell(self, r: int, c: int) -> None:
        self._selected_cell = (r, c)
        self._selected_edge = None
        self._selected_vertex = None
        self._rebuild_cell()

    def select_edge(self, r1: int, c1: int, r2: int, c2: int) -> None:
        self._selected_edge = (r1, c1, r2, c2)
        self._selected_cell = None
        self._selected_vertex = None
        self._rebuild_edge()

    def select_vertex(self, r: int, c: int) -> None:
        self._selected_vertex = (r, c)
        self._selected_cell = None
        self._selected_edge = None
        self._rebuild_vertex()

    # ── Cell panel ──────────────────────────────────────────────

    def _rebuild_cell(self) -> None:
        if self._board is None or self._selected_cell is None:
            return
        r, c = self._selected_cell
        cell = self._board.cell(r, c)
        self._info.setText(f'<b style="font-size:13px;">单元格 ({r}, {c})</b>')
        self._clear_content()

        L = self._content

        # ── Blocked ──
        cb = QCheckBox("障碍格")
        cb.setChecked(cell.blocked)
        cb.toggled.connect(lambda checked: self._set_cell_blocked(r, c, checked))
        L.addWidget(cb)
        _sep(L)

        # ── Number ──
        nr = QHBoxLayout()
        nr.setSpacing(4)
        nr.addWidget(QLabel("数字"))
        ns = QSpinBox()
        ns.setRange(0, 999)
        ns.setValue(cell.number or 0)
        ns.valueChanged.connect(lambda v: self._set_cell_number(r, c, v if v > 0 else None))
        nr.addWidget(ns)
        ncl = QPushButton("清除")
        ncl.setFixedWidth(40)
        ncl.setStyleSheet("QPushButton { font-size: 10px; padding: 2px 4px; }")
        ncl.clicked.connect(lambda: (ns.setValue(0), self._set_cell_number(r, c, None)))
        nr.addWidget(ncl)
        nr.addStretch()
        L.addLayout(nr)

        # ── Symbol ──
        sr = QHBoxLayout()
        sr.setSpacing(4)
        sr.addWidget(QLabel("符号"))
        si = QLineEdit()
        si.setMaxLength(2)
        si.setFixedWidth(44)
        si.setText(cell.symbol or "")
        si.textChanged.connect(lambda t: self._set_cell_symbol(r, c, t or None))
        sr.addWidget(si)
        for sym in ["★", "●", "◆", "▲", "♥", "■"]:
            b = QPushButton(sym)
            b.setFixedSize(28, 24)
            b.setStyleSheet("QPushButton { font-size: 13px; padding: 1px; border-radius: 3px; }")
            b.clicked.connect(lambda checked, s=sym: (si.setText(s), self._set_cell_symbol(r, c, s)))
            sr.addWidget(b)
        scl = QPushButton("×")
        scl.setFixedWidth(22)
        scl.clicked.connect(lambda: (si.clear(), self._set_cell_symbol(r, c, None)))
        sr.addWidget(scl)
        L.addLayout(sr)

        # ── Compass ──
        _sep(L)
        compass_label = QLabel("罗盘")
        compass_label.setStyleSheet("font-weight: bold; font-size: 11px;")
        L.addWidget(compass_label)

        cg = QGridLayout()
        cg.setSpacing(3)
        val = type("v", (), {"__getitem__": lambda s, k: -1})()

        def mk_spin(init_val: int) -> QSpinBox:
            s = QSpinBox()
            s.setRange(-1, 99)
            s.setValue(init_val)
            s.setFixedWidth(64)
            s.setAlignment(Qt.AlignmentFlag.AlignCenter)
            return s

        cp = cell.compass
        su = mk_spin(cp.up if cp else -1)
        sd = mk_spin(cp.down if cp else -1)
        sl = mk_spin(cp.left if cp else -1)
        sr2 = mk_spin(cp.right if cp else -1)

        cg.addWidget(QLabel("  "), 0, 0)
        cg.addWidget(su, 0, 1)
        cg.addWidget(QLabel("  "), 0, 2)
        cg.addWidget(sl, 1, 0)
        center = QLabel("●")
        center.setAlignment(Qt.AlignmentFlag.AlignCenter)
        center.setStyleSheet("font-size: 14px;")
        cg.addWidget(center, 1, 1)
        cg.addWidget(sr2, 1, 2)
        cg.addWidget(QLabel("  "), 2, 0)
        cg.addWidget(sd, 2, 1)
        L.addLayout(cg)

        def emit_compass():
            from src.models.board import CompassClue
            clue = CompassClue(
                up=su.value() if su.value() >= 0 else -1,
                down=sd.value() if sd.value() >= 0 else -1,
                left=sl.value() if sl.value() >= 0 else -1,
                right=sr2.value() if sr2.value() >= 0 else -1,
            )
            self._board.cell(r, c).compass = clue
            self.board_modified.emit()

        apply_c = QPushButton("应用罗盘")
        apply_c.setStyleSheet("QPushButton { font-size: 11px; padding: 3px 8px; }")
        apply_c.clicked.connect(emit_compass)
        L.addWidget(apply_c)

        # ── Patterns ──
        _sep(L)
        patterns_label = QLabel("图案")
        patterns_label.setStyleSheet("font-weight: bold; font-size: 11px;")
        L.addWidget(patterns_label)

        from src.ui.shape_editor import PatternEditorDialog

        def _edit_shape_pattern():
            dialog = PatternEditorDialog(self, existing=cell.shape_pattern, title="拼块图案编辑")
            if dialog.exec():
                shape = dialog.get_shape()
                self._board.cell(r, c).shape_pattern = shape if shape.area > 0 else None
                self.board_modified.emit()
                self._rebuild_cell()

        def _edit_fence_pattern():
            dialog = PatternEditorDialog(self, existing=cell.fence_pattern, title="围栏标记编辑")
            if dialog.exec():
                shape = dialog.get_shape()
                self._board.cell(r, c).fence_pattern = shape if shape.area > 0 else None
                self.board_modified.emit()
                self._rebuild_cell()

        has_sp = cell.shape_pattern is not None
        sp_btn = QPushButton(f"拼块图案 {'有' if has_sp else '无'}")
        sp_btn.setStyleSheet("QPushButton { font-size: 11px; padding: 4px 8px; }")
        sp_btn.clicked.connect(lambda: _edit_shape_pattern())
        L.addWidget(sp_btn)

        has_fp = cell.fence_pattern is not None
        fp_btn = QPushButton(f"围栏标记 {'有' if has_fp else '无'}")
        fp_btn.setStyleSheet("QPushButton { font-size: 11px; padding: 4px 8px; }")
        fp_btn.clicked.connect(lambda: _edit_fence_pattern())
        L.addWidget(fp_btn)

    # ── Cell setters ──

    def _set_cell_blocked(self, r: int, c: int, blocked: bool) -> None:
        cell = self._board.cell(r, c)
        cell.blocked = blocked
        if blocked:
            cell.number = None
            cell.symbol = None
            cell.compass = None
            cell.shape_pattern = None
            cell.fence_pattern = None
            cell.region_id = None
        self.board_modified.emit()

    def _set_cell_number(self, r: int, c: int, value: int | None) -> None:
        self._board.cell(r, c).number = value
        self.board_modified.emit()

    def _set_cell_symbol(self, r: int, c: int, value: str | None) -> None:
        self._board.cell(r, c).symbol = value
        self.board_modified.emit()

    # ── Edge panel ──────────────────────────────────────────────

    def _rebuild_edge(self) -> None:
        if self._board is None or self._selected_edge is None:
            return
        r1, c1, r2, c2 = self._selected_edge
        e = self._board.edge_between(r1, c1, r2, c2)
        if e is None:
            return

        constraint_str = "无"
        if e.constraint is not None:
            ct = e.constraint.type.value
            val = f", 值={e.constraint.value}" if e.constraint.value is not None else ""
            constraint_str = f"{ct}{val}"

        self._info.setText(
            '<b style="font-size:13px;">边框 ({},{})-({},{})</b><br>'
            '<span>分割:</span> {}<br>'
            '<span>约束:</span> {}'.format(
                r1, c1, r2, c2, "是" if e.is_boundary else "否", constraint_str
            )
        )
        self._clear_content()
        L = self._content

        btn_style = "QPushButton { font-size: 11px; padding: 5px 8px; border-radius: 4px; }"

        # ── Boundary toggle ──
        if e.is_boundary:
            tb = QPushButton("取消分割线")
            tb.setStyleSheet(
                "QPushButton { font-size: 12px; padding: 6px; border-radius: 5px; font-weight: bold; "
                "color: #EF4444; border: 1px solid #EF4444; }"
                "QPushButton:hover { background: #3D1F1F; }"
            )
        else:
            tb = QPushButton("设为分割线")
            tb.setStyleSheet(
                "QPushButton { font-size: 12px; padding: 6px; border-radius: 5px; font-weight: bold; "
                "color: #3B82F6; border: 1px solid #3B82F6; }"
                "QPushButton:hover { background: #1E3A5F; }"
            )
        tb.clicked.connect(self._toggle_boundary)
        L.addWidget(tb)
        _sep(L)

        # ── Hetero / Homo ──
        ca = e.constraint is not None
        hrow = QHBoxLayout()
        hrow.setSpacing(4)
        bh = QPushButton("≠异生")
        bh.setStyleSheet(btn_style)
        bh.setCheckable(True)
        bh.setChecked(ca and e.constraint.type == EdgeConstraintType.HETEROGENEOUS)
        bh.clicked.connect(lambda: self._set_edge_constraint(EdgeConstraintType.HETEROGENEOUS))
        hrow.addWidget(bh)
        bm = QPushButton("=双生")
        bm.setStyleSheet(btn_style)
        bm.setCheckable(True)
        bm.setChecked(ca and e.constraint.type == EdgeConstraintType.HOMOGENEOUS)
        bm.clicked.connect(lambda: self._set_edge_constraint(EdgeConstraintType.HOMOGENEOUS))
        hrow.addWidget(bm)
        L.addLayout(hrow)

        # ── Inequality direction ──
        ineq_rev = e.constraint.value == 1 if ca and e.constraint.type == EdgeConstraintType.INEQUALITY else False
        is_vert = e.c1 == e.c2
        dirs = [("↑上大下小", 0), ("↓下大上小", 1)] if is_vert else [("←左大右小", 0), ("→右大左小", 1)]
        irow = QHBoxLayout()
        irow.setSpacing(4)
        irow.addWidget(QLabel("不等"))
        for label, val in dirs:
            b = QPushButton(label)
            b.setStyleSheet(btn_style)
            b.setCheckable(True)
            b.setChecked(ca and e.constraint.type == EdgeConstraintType.INEQUALITY and ineq_rev == (val == 1))
            b.clicked.connect(lambda checked, v=val: self._set_inequality(v))
            irow.addWidget(b)
        irow.addStretch()
        L.addLayout(irow)

        # ── Difference ──
        drow = QHBoxLayout()
        drow.setSpacing(4)
        drow.addWidget(QLabel("差值"))
        ds = QSpinBox()
        ds.setRange(1, 999)
        ds.setValue(e.constraint.value if ca and e.constraint.type == EdgeConstraintType.DIFFERENCE else 1)
        drow.addWidget(ds)
        da = QPushButton("设差值")
        da.setStyleSheet(btn_style)
        da.clicked.connect(lambda: self._set_edge_constraint(EdgeConstraintType.DIFFERENCE, ds.value()))
        drow.addWidget(da)
        drow.addStretch()
        L.addLayout(drow)

        # ── Clear ──
        _sep(L)
        cl = QPushButton("清除约束")
        cl.setStyleSheet(
            "QPushButton { font-size: 11px; padding: 5px; border-radius: 4px; color: #EF4444; }"
            "QPushButton:hover { background: #3D1F1F; }"
        )
        cl.clicked.connect(self._clear_edge_constraint)
        L.addWidget(cl)

    def _toggle_boundary(self) -> None:
        if self._board is None or self._selected_edge is None:
            return
        e = self._board.edge_between(*self._selected_edge)
        if e is not None:
            e.is_boundary = not e.is_boundary
            self._rebuild_edge()
            self.board_modified.emit()

    def _set_inequality(self, reverse: int) -> None:
        self._set_edge_constraint(EdgeConstraintType.INEQUALITY, reverse)

    def _set_edge_constraint(self, ctype: EdgeConstraintType, value: int | None = None) -> None:
        if self._board is None or self._selected_edge is None:
            return
        e = self._board.edge_between(*self._selected_edge)
        if e is not None:
            e.constraint = EdgeConstraint(type=ctype, value=value)
            self._rebuild_edge()
            self.board_modified.emit()

    def _clear_edge_constraint(self) -> None:
        if self._board is None or self._selected_edge is None:
            return
        e = self._board.edge_between(*self._selected_edge)
        if e is not None:
            e.constraint = None
            self._rebuild_edge()
            self.board_modified.emit()

    # ── Vertex panel ────────────────────────────────────────────

    def _rebuild_vertex(self) -> None:
        if self._board is None or self._selected_vertex is None:
            return
        vr, vc = self._selected_vertex
        v = self._board.vertex_at(vr, vc)
        if v is None:
            return
        self._info.setText(
            '<b style="font-size:13px;">顶点 ({},{})</b><br>'
            '<span>望塔:</span> {}'.format(
                vr, vc, v.watchtower if v.watchtower is not None else "无"
            )
        )
        self._clear_content()
        L = self._content

        wrow = QHBoxLayout()
        wrow.addWidget(QLabel("望塔值"))
        ws = QSpinBox()
        ws.setRange(0, 999)
        ws.setValue(v.watchtower if v.watchtower is not None else 0)
        wrow.addWidget(ws)
        wa = QPushButton("设置")
        wa.setStyleSheet("QPushButton { font-size: 11px; padding: 4px 8px; }")
        wa.clicked.connect(lambda: self._set_watchtower(vr, vc, ws.value() if ws.value() > 0 else None))
        wrow.addWidget(wa)
        wcl = QPushButton("清除")
        wcl.setStyleSheet("QPushButton { font-size: 11px; padding: 4px 8px; color: #DC2626; }")
        wcl.clicked.connect(lambda: (ws.setValue(0), self._set_watchtower(vr, vc, None)))
        wrow.addWidget(wcl)
        wrow.addStretch()
        L.addLayout(wrow)

    def _set_watchtower(self, r: int, c: int, value: int | None) -> None:
        v = self._board.vertex_at(r, c)
        if v is not None:
            v.watchtower = value
            self.board_modified.emit()
