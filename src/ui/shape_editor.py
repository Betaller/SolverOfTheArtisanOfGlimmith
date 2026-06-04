from __future__ import annotations

from typing import Optional

from PySide6.QtCore import Qt, QRectF
from PySide6.QtGui import QPainter, QPen, QBrush, QColor, QMouseEvent
from PySide6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QGridLayout,
    QPushButton, QLabel, QWidget, QListWidget,
    QListWidgetItem, QGroupBox, QFrame,
)

from src.models.board import Shape
from src.solver.shapes import normalize, canonical_key


CELL_SIZE = 36


class ShapeGridEditor(QWidget):
    def __init__(self, grid_size: int = 5, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.grid_size = grid_size
        self._cells: set[tuple[int, int]] = set()
        self.setMinimumSize(grid_size * CELL_SIZE + 2, grid_size * CELL_SIZE + 26)
        self.setMouseTracking(True)
        self._painting = False

    def set_cells(self, cells: set[tuple[int, int]]) -> None:
        self._cells = {(r, c) for r, c in cells if 0 <= r < self.grid_size and 0 <= c < self.grid_size}
        self.update()

    def get_shape(self) -> Shape:
        return Shape(cells=normalize(frozenset(self._cells)))

    def clear(self) -> None:
        self._cells.clear()
        self.update()

    def _cell_at(self, pos) -> tuple[int, int] | None:
        x, y = pos.x(), pos.y()
        c = int(x // CELL_SIZE)
        r = int(y // CELL_SIZE)
        if 0 <= r < self.grid_size and 0 <= c < self.grid_size:
            return (r, c)
        return None

    def mousePressEvent(self, event: QMouseEvent) -> None:
        cell = self._cell_at(event.position())
        if cell is not None:
            self._painting = True
            if cell in self._cells:
                self._cells.remove(cell)
            else:
                self._cells.add(cell)
            self.update()

    def mouseMoveEvent(self, event: QMouseEvent) -> None:
        if self._painting:
            cell = self._cell_at(event.position())
            if cell is not None:
                self._cells.add(cell)
                self.update()

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:
        self._painting = False

    def paintEvent(self, event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        for r in range(self.grid_size):
            for c in range(self.grid_size):
                x = c * CELL_SIZE
                y = r * CELL_SIZE
                rect = QRectF(x, y, CELL_SIZE, CELL_SIZE)

                if (r, c) in self._cells:
                    painter.fillRect(rect, QColor("#3B82F6"))
                    painter.setPen(QPen(QColor("#2563EB"), 1))
                else:
                    painter.fillRect(rect, QColor("#F8FAFC"))
                    painter.setPen(QPen(QColor("#D0D5DD"), 1))

                painter.drawRoundedRect(rect.adjusted(0.5, 0.5, -0.5, -0.5), 2, 2)

        area = len(self._cells)
        painter.setPen(QPen(QColor("#475569")))
        painter.setFont(self.font())
        painter.drawText(QRectF(0, self.grid_size * CELL_SIZE + 6,
                                  self.grid_size * CELL_SIZE, 18),
                         Qt.AlignmentFlag.AlignCenter, f"格数: {area}")


class ShapeEditorDialog(QDialog):
    def __init__(self, parent: QWidget | None = None,
                 existing_shapes: list[Shape] | None = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("形状池编辑器")
        self.resize(520, 520)
        self.setMinimumSize(480, 460)

        self._shapes: list[Shape] = list(existing_shapes) if existing_shapes else []

        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 16, 16, 16)
        layout.setSpacing(12)

        editor_group = QGroupBox("绘制形状（点击/拖动填充格子）")
        editor_group.setStyleSheet("""
            QGroupBox {
                font-size: 13px; font-weight: bold; color: #1E293B;
                border: 1px solid #E0E3E8; border-radius: 8px;
                margin-top: 12px; padding: 16px 12px 12px;
                background: #FFFFFF;
            }
            QGroupBox::title {
                subcontrol-origin: margin;
                subcontrol-position: top left;
                left: 12px; padding: 0 6px;
            }
        """)
        editor_layout = QVBoxLayout(editor_group)
        editor_layout.setSpacing(8)

        self._editor = ShapeGridEditor(grid_size=6)

        editor_btn_row = QHBoxLayout()
        clear_btn = QPushButton("清空")
        clear_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        clear_btn.clicked.connect(self._editor.clear)
        add_btn = QPushButton("添加到形状池")
        add_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        add_btn.setStyleSheet(
            "QPushButton { background: #3B82F6; color: white; border: none; }"
            "QPushButton:hover { background: #2563EB; }"
        )
        add_btn.clicked.connect(self._add_shape)
        editor_btn_row.addWidget(clear_btn)
        editor_btn_row.addWidget(add_btn)
        editor_btn_row.addStretch()

        editor_layout.addWidget(self._editor, alignment=Qt.AlignmentFlag.AlignCenter)
        editor_layout.addLayout(editor_btn_row)
        layout.addWidget(editor_group)

        pool_group = QGroupBox("形状池列表")
        pool_group.setStyleSheet("""
            QGroupBox {
                font-size: 13px; font-weight: bold; color: #1E293B;
                border: 1px solid #E0E3E8; border-radius: 8px;
                margin-top: 12px; padding: 16px 12px 12px;
                background: #FFFFFF;
            }
            QGroupBox::title {
                subcontrol-origin: margin;
                subcontrol-position: top left;
                left: 12px; padding: 0 6px;
            }
        """)
        pool_layout = QVBoxLayout(pool_group)
        pool_layout.setSpacing(8)

        self._shape_list = QListWidget()
        self._refresh_list()
        pool_layout.addWidget(self._shape_list)

        pool_btn_row = QHBoxLayout()
        remove_btn = QPushButton("删除选中")
        remove_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        remove_btn.clicked.connect(self._remove_shape)
        pool_btn_row.addWidget(remove_btn)
        pool_btn_row.addStretch()
        pool_layout.addLayout(pool_btn_row)

        layout.addWidget(pool_group)

        btn_row = QHBoxLayout()
        btn_row.addStretch()
        ok_btn = QPushButton("确定")
        ok_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        ok_btn.setStyleSheet(
            "QPushButton { background: #10B981; color: white; border: none; padding: 8px 24px; }"
            "QPushButton:hover { background: #059669; }"
        )
        ok_btn.clicked.connect(self.accept)
        cancel_btn = QPushButton("取消")
        cancel_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        cancel_btn.clicked.connect(self.reject)
        btn_row.addWidget(ok_btn)
        btn_row.addWidget(cancel_btn)
        layout.addLayout(btn_row)

    def _add_shape(self) -> None:
        shape = self._editor.get_shape()
        if shape.area == 0:
            return
        for existing in self._shapes:
            if canonical_key(existing.cells) == canonical_key(shape.cells):
                return
        self._shapes.append(shape)
        self._editor.clear()
        self._refresh_list()

    def _remove_shape(self) -> None:
        idx = self._shape_list.currentRow()
        if 0 <= idx < len(self._shapes):
            self._shapes.pop(idx)
            self._refresh_list()

    def _refresh_list(self) -> None:
        self._shape_list.clear()
        for i, s in enumerate(self._shapes):
            cells_desc = ", ".join(f"({r},{c})" for r, c in sorted(s.cells))
            self._shape_list.addItem(f"形状{i+1} (面积={s.area}): [{cells_desc}]")

    def get_shapes(self) -> list[Shape]:
        return list(self._shapes)


class PatternEditorDialog(QDialog):
    def __init__(self, parent: QWidget | None = None,
                 existing: Shape | None = None,
                 title: str = "图案编辑器") -> None:
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setFixedSize(300, 360)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 16, 16, 16)
        layout.setSpacing(12)

        self._editor = ShapeGridEditor(grid_size=5)
        if existing is not None and existing.cells:
            self._editor.set_cells(set(existing.cells))
        layout.addWidget(self._editor, alignment=Qt.AlignmentFlag.AlignCenter)

        btn_row = QHBoxLayout()
        clear_btn = QPushButton("清空")
        clear_btn.clicked.connect(self._editor.clear)
        btn_row.addWidget(clear_btn)
        btn_row.addStretch()

        ok_btn = QPushButton("确定")
        ok_btn.setStyleSheet(
            "QPushButton { background: #10B981; color: white; border: none; padding: 6px 20px; }"
            "QPushButton:hover { background: #059669; }"
        )
        ok_btn.clicked.connect(self.accept)
        cancel_btn = QPushButton("取消")
        cancel_btn.clicked.connect(self.reject)
        btn_row.addWidget(ok_btn)
        btn_row.addWidget(cancel_btn)
        layout.addLayout(btn_row)

    def get_shape(self) -> Shape:
        return self._editor.get_shape()
