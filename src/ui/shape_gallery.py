"""形状画廊：展示指定大小的所有不同形状（忽略旋转/翻转）。

配合“相异 (different)”规则使用——所有区域形状必须互不相同，本工具列出
给定大小下所有可能的形状，便于设计与核对。
"""
from __future__ import annotations

from typing import Optional

from PySide6.QtCore import Qt, QRectF, QSize, QEvent
from PySide6.QtGui import QPainter, QPen, QBrush, QColor, QFont
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QComboBox, QScrollArea,
    QDialog,
)

from src.models.puzzle import Puzzle
from src.solver.shapes import enumerate_polyominoes
from src.ui import theme as _ui_theme


class ShapeGridView(QWidget):
    """Paints all shapes in a wrapped grid, sized so nothing overlaps.

    The slot size is derived from the widest/tallest shape of the current set
    and the cell is scaled down for larger shapes, so rows reflow on resize.
    """

    GAP = 3
    PAD = 10

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._shapes: list = []
        self._cell = 16
        self._slot = 24

    def set_shapes(self, shapes: list) -> None:
        self._shapes = list(shapes)
        max_dim = 1
        if shapes:
            for s in shapes:
                rs = [r for r, _ in s.cells]
                cs = [c for _, c in s.cells]
                w = max(cs) - min(cs) + 1 if cs else 1
                h = max(rs) - min(rs) + 1 if rs else 1
                max_dim = max(max_dim, w, h)
        self._cell = max(8, min(18, 100 // max_dim))
        self._slot = max_dim * (self._cell + self.GAP) + 12  # + index gutter
        self._update_height()
        self.update()

    def _cols(self) -> int:
        return max(1, (self.width() - 2 * self.PAD) // max(1, self._slot))

    def _update_height(self) -> None:
        if not self._shapes:
            self.setMinimumHeight(60)
            self.setMaximumHeight(16777215)
            return
        cols = self._cols()
        rows = (len(self._shapes) + cols - 1) // cols
        h = rows * self._slot + 2 * self.PAD
        self.setMinimumHeight(h)
        self.setMaximumHeight(h)

    def resizeEvent(self, event) -> None:
        super().resizeEvent(event)
        self._update_height()

    def paintEvent(self, event) -> None:
        super().paintEvent(event)
        if not self._shapes:
            return
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.fillRect(self.rect(), QColor(_ui_theme.colors.preview_bg))

        pen = QPen(QColor(_ui_theme.colors.shape_mini_pen), 1)
        fill = QBrush(QColor(_ui_theme.colors.shape_mini_fill))
        label_font = QFont("Segoe UI", max(7, self._cell // 2))
        step = self._cell + self.GAP
        cols = self._cols()

        for i, shape in enumerate(self._shapes, 1):
            rs = [r for r, _ in shape.cells]
            cs = [c for _, c in shape.cells]
            min_r, max_r = min(rs), max(rs)
            min_c, max_c = min(cs), max(cs)
            w = max_c - min_c + 1
            h = max_r - min_r + 1
            col = (i - 1) % cols
            row = (i - 1) // cols
            slot_x = self.PAD + col * self._slot
            slot_y = self.PAD + row * self._slot
            ox = slot_x + (self._slot - w * step) / 2
            oy = slot_y + (self._slot - h * step) / 2

            p.setPen(pen)
            p.setBrush(fill)
            for r, c in shape.cells:
                px = ox + (c - min_c) * step
                py = oy + (r - min_r) * step
                p.drawRoundedRect(QRectF(px, py, self._cell, self._cell), 1, 1)

            p.setFont(label_font)
            p.setPen(QColor(_ui_theme.colors.preview_summary_text))
            p.drawText(QRectF(slot_x + 2, slot_y, self._slot - 4, 12),
                       Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignTop,
                       str(i))
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

        self._view = ShapeGridView()
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setWidget(self._view)
        layout.addWidget(scroll, 1)

    def _populate(self) -> None:
        n = int(self._size_combo.currentText())
        self._size = n
        self._view.set_shapes(enumerate_polyominoes(n))
        self._count_label.setText(f"共 {len(enumerate_polyominoes(n))} 种")

    def current_size(self) -> int:
        return self._size

    def set_puzzle(self, puzzle: Puzzle) -> None:
        """Auto-select a size relevant to the loaded puzzle."""
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


class ShapeGalleryDialog(QDialog):
    """独立窗口：按大小网格展示所有不同形状（忽略旋转/翻转）。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("形状画廊 - 相异 (different) 规则")
        self.resize(440, 540)
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

        hint = QLabel("所有不同形状（忽略旋转/翻转），网格排列。")
        hint.setStyleSheet("font-size: 11px; color: #777;")
        layout.addWidget(hint)

        self._view = ShapeGridView()
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setWidget(self._view)
        layout.addWidget(scroll, 1)

    def _populate(self) -> None:
        n = int(self._size_combo.currentText())
        shapes = enumerate_polyominoes(n)
        self._view.set_shapes(shapes)
        self._count_label.setText(f"共 {len(shapes)} 种")
