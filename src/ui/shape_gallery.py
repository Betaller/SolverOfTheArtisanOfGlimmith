"""形状画廊：展示指定大小的所有不同形状（忽略旋转/翻转）。

配合“相异 (different)”规则使用——所有区域形状必须互不相同，本工具列出
给定大小下所有可能的形状，便于设计与核对。
"""
from __future__ import annotations

from typing import Optional

from PySide6.QtCore import Qt, QRectF, QSize
from PySide6.QtGui import QPainter, QPen, QBrush, QColor, QFont
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QComboBox, QScrollArea,
    QDialog,
)

from src.models.puzzle import Puzzle
from src.solver.shapes import enumerate_polyominoes
from src.ui import theme as _ui_theme


class ShapeGalleryView(QWidget):
    """Paints all shapes of the current size in a wrapped grid."""

    CELL = 14
    GAP = 2
    PAD = 8

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._shapes: list = []

    def set_shapes(self, shapes: list) -> None:
        self._shapes = list(shapes)
        self.updateGeometry()
        self.update()

    def _shape_metrics(self, shape):
        rs = [r for r, _ in shape.cells]
        cs = [c for _, c in shape.cells]
        return min(rs), max(rs), min(cs), max(cs)

    def minimumSizeHint(self) -> QSize:
        return self.sizeHint()

    def sizeHint(self) -> QSize:
        if not self._shapes:
            return QSize(200, 60)
        # layout metrics
        cell_w = self.CELL + self.GAP
        shape_w = cell_w * 6 + self.PAD * 2
        rows, x = 1, 0
        for shape in self._shapes:
            _, _, min_c, max_c = self._shape_metrics(shape)
            w = (max_c - min_c + 1) * cell_w + self.PAD * 2 + 14  # + label
            if x + w > 240:
                rows += 1
                x = w
            else:
                x += w
        return QSize(260, rows * (self.CELL * 6 + self.PAD * 2) + 8)

    def paintEvent(self, event) -> None:
        super().paintEvent(event)
        if not self._shapes:
            return
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.fillRect(self.rect(), QColor(_ui_theme.colors.preview_bg))

        pen = QPen(QColor(_ui_theme.colors.shape_mini_pen), 1)
        fill = QBrush(QColor(_ui_theme.colors.shape_mini_fill))
        font = QFont("Segoe UI", 8)
        cell_w = self.CELL + self.GAP
        max_span = 6  # columns per shape slot

        x0, y0 = self.PAD, self.PAD
        cur_x, cur_y = x0, y0
        for i, shape in enumerate(self._shapes, 1):
            min_r, max_r, min_c, max_c = self._shape_metrics(shape)
            h = max_r - min_r + 1
            w = max_c - min_c + 1
            slot_w = max_span * cell_w + self.PAD * 2 + 14
            if cur_x + slot_w > self.width() - self.PAD:
                cur_x = x0
                cur_y += max_span * cell_w + self.PAD
            ox = cur_x + (slot_w - w * cell_w) / 2
            oy = cur_y + (slot_w - h * cell_w) / 2
            p.setPen(pen)
            p.setBrush(fill)
            for r, c in shape.cells:
                px = ox + (c - min_c) * cell_w
                py = oy + (r - min_r) * cell_w
                p.drawRoundedRect(QRectF(px, py, self.CELL, self.CELL), 1, 1)
            p.setPen(QColor(_ui_theme.colors.preview_summary_text))
            p.setFont(font)
            p.drawText(QRectF(cur_x + slot_w - 16, cur_y, 14, 12),
                       Qt.AlignmentFlag.AlignCenter, str(i))
            cur_x += slot_w

        p.end()


class ShapeGalleryWidget(QWidget):
    """Tool: pick a size (1-7) and see every distinct shape of that size."""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._size = 1
        self._setup_ui()
        self._populate()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(6, 6, 6, 6)
        layout.setSpacing(6)

        top = QHBoxLayout()
        top.addWidget(QLabel("大小"))
        self._size_combo = QComboBox()
        for n in range(1, 8):
            self._size_combo.addItem(str(n))
        self._size_combo.currentIndexChanged.connect(self._populate)
        self._size_combo.setStyleSheet(
            "QComboBox { padding: 4px 6px; border-radius: 4px; font-size: 12px; }"
        )
        top.addWidget(self._size_combo)
        self._count_label = QLabel("")
        self._count_label.setStyleSheet("font-size: 12px; color: #555;")
        top.addWidget(self._count_label)
        top.addStretch()
        layout.addLayout(top)

        hint = QLabel("按大小列出所有不同的形状（忽略旋转/翻转），"
                      "用于“相异”等形状规则。")
        hint.setWordWrap(True)
        hint.setStyleSheet("font-size: 11px; color: #777;")
        layout.addWidget(hint)

        self._view = ShapeGalleryView()
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setWidget(self._view)
        layout.addWidget(scroll, 1)

    def _populate(self) -> None:
        n = int(self._size_combo.currentText())
        self._size = n
        shapes = enumerate_polyominoes(n)
        self._view.set_shapes(shapes)
        self._count_label.setText(f"共 {len(shapes)} 种")

    def current_size(self) -> int:
        return self._size

    def set_puzzle(self, puzzle: Puzzle) -> None:
        """Auto-select a size relevant to the loaded puzzle.

        For the “different” rule this is the region size when it can be
        determined from precise / range / area / shape-pool rules.
        """
        size = self._region_size(puzzle)
        if size is not None and 1 <= size <= 7:
            self._size_combo.blockSignals(True)
            self._size_combo.setCurrentIndex(size - 1)
            self._size_combo.blockSignals(False)
            self._populate()

    @staticmethod
    def _region_size(puzzle: Puzzle) -> int | None:
        precise = puzzle.get_rule("precise")
        if precise is not None:
            return int(precise.params.get("area", 0)) or None
        range_rule = puzzle.get_rule("range")
        if range_rule is not None:
            lo = range_rule.params.get("min")
            hi = range_rule.params.get("max")
            if lo is not None and lo == hi:
                return int(lo)
        area_rule = puzzle.get_rule("area")
        if area_rule is not None:
            nums = {c.number for c in puzzle.cells if c.number is not None}
            if len(nums) == 1:
                return next(iter(nums))
        pool = puzzle.get_rule("shape_pool")
        if pool is not None:
            areas = {s.area for s in pool.params.get("shapes", [])}
            if len(areas) == 1:
                return next(iter(areas))
        return None


class VerticalShapeList(QWidget):
    """Paints all shapes stacked from top to bottom, one per row."""

    CELL = 16
    GAP = 2
    ROW_H = 46

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._shapes: list = []

    def set_shapes(self, shapes: list) -> None:
        self._shapes = list(shapes)
        self.updateGeometry()
        self.update()

    def sizeHint(self) -> QSize:
        return QSize(300, max(60, len(self._shapes) * self.ROW_H))

    def minimumSizeHint(self) -> QSize:
        return self.sizeHint()

    def paintEvent(self, event) -> None:
        super().paintEvent(event)
        if not self._shapes:
            return
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.fillRect(self.rect(), QColor(_ui_theme.colors.preview_bg))
        pen = QPen(QColor(_ui_theme.colors.shape_mini_pen), 1)
        fill = QBrush(QColor(_ui_theme.colors.shape_mini_fill))
        label_font = QFont("Segoe UI", 9)
        cell_w = self.CELL + self.GAP

        for i, shape in enumerate(self._shapes, 1):
            y = (i - 1) * self.ROW_H
            rs = [r for r, _ in shape.cells]
            cs = [c for _, c in shape.cells]
            min_r, max_r = min(rs), max(rs)
            min_c, max_c = min(cs), max(cs)
            h = max_r - min_r + 1
            w = max_c - min_c + 1
            ox = 44 + (self.width() - 44 - w * cell_w) / 2
            oy = y + (self.ROW_H - h * cell_w) / 2
            p.setPen(pen)
            p.setBrush(fill)
            for r, c in shape.cells:
                px = ox + (c - min_c) * cell_w
                py = oy + (r - min_r) * cell_w
                p.drawRoundedRect(QRectF(px, py, self.CELL, self.CELL), 1, 1)
            p.setFont(label_font)
            p.setPen(QColor(_ui_theme.colors.preview_summary_text))
            p.drawText(QRectF(8, y, 30, self.ROW_H),
                       Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft, f"{i}.")
        p.end()


class ShapeGalleryDialog(QDialog):
    """独立窗口：按大小从上到下列出所有不同形状（忽略旋转/翻转）。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("形状画廊 - 相异 (different) 规则")
        self.resize(340, 520)
        self._setup_ui()
        self._populate()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(6)

        top = QHBoxLayout()
        top.addWidget(QLabel("大小"))
        self._size_combo = QComboBox()
        for n in range(1, 8):
            self._size_combo.addItem(str(n))
        self._size_combo.currentIndexChanged.connect(self._populate)
        self._size_combo.setStyleSheet(
            "QComboBox { padding: 4px 6px; border-radius: 4px; font-size: 12px; }"
        )
        top.addWidget(self._size_combo)
        self._count_label = QLabel("")
        self._count_label.setStyleSheet("font-size: 12px; color: #555;")
        top.addWidget(self._count_label)
        top.addStretch()
        layout.addLayout(top)

        hint = QLabel("所有不同形状（忽略旋转/翻转），从上到下依次展示。")
        hint.setStyleSheet("font-size: 11px; color: #777;")
        layout.addWidget(hint)

        self._list = VerticalShapeList()
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setWidget(self._list)
        layout.addWidget(scroll, 1)

    def _populate(self) -> None:
        n = int(self._size_combo.currentText())
        shapes = enumerate_polyominoes(n)
        self._list.set_shapes(shapes)
        self._count_label.setText(f"共 {len(shapes)} 种")
