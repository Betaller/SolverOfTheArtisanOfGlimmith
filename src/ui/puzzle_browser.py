from __future__ import annotations

import json
import os
import glob
from typing import Optional

from PySide6.QtCore import Qt, Signal, QRectF, QPointF
from PySide6.QtGui import (
    QPainter, QPen, QBrush, QColor, QFont, QPixmap,
)
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLineEdit, QComboBox,
    QListWidget, QListWidgetItem, QLabel, QSplitter, QFrame,
)

from src.models.puzzle import RULE_NAMES


PUZZLE_BASE = "puzzles"


class PuzzleInfo:
    __slots__ = ("name", "path", "category", "height", "width",
                 "rules", "blocked_count", "has_boundaries")

    def __init__(self, name: str, path: str, category: str,
                 height: int, width: int, rules: list[str],
                 blocked_count: int, has_boundaries: bool) -> None:
        self.name = name
        self.path = path
        self.category = category
        self.height = height
        self.width = width
        self.rules = rules
        self.blocked_count = blocked_count
        self.has_boundaries = has_boundaries

    def rule_display(self) -> str:
        return ", ".join(RULE_NAMES.get(r, r) for r in self.rules) if self.rules else "无规则"


class PuzzlePreviewWidget(QWidget):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._info: PuzzleInfo | None = None
        self._grid_data: dict | None = None
        self.setMinimumSize(200, 160)
        self.setMaximumHeight(200)

    def set_puzzle(self, info: PuzzleInfo, grid_data: dict) -> None:
        self._info = info
        self._grid_data = grid_data
        self.update()

    def clear(self) -> None:
        self._info = None
        self._grid_data = None
        self.update()

    def paintEvent(self, event) -> None:
        super().paintEvent(event)
        if self._info is None or self._grid_data is None:
            return

        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.fillRect(self.rect(), QColor("#FFFFFF"))

        info = self._info
        gw = info.width
        gh = info.height
        if gw <= 0 or gh <= 0:
            return

        # Compute cell size to fit available space
        margin = 8
        avail_w = self.width() - margin * 2
        avail_h = self.height() - margin * 2
        cell_size = max(4, min(avail_w // max(gw, 1), avail_h // max(gh, 1)))
        cell_size = min(cell_size, 14)

        total_w = gw * cell_size
        total_h = gh * cell_size
        ox = margin + (avail_w - total_w) / 2
        oy = margin + (avail_h - total_h) / 2

        blocked_set: set[tuple[int, int]] = set()
        boundary_set: set[tuple[int, int, int, int]] = set()
        has_numbers = False
        has_symbols = False

        grid = self._grid_data.get("grid", {})
        cells = self._grid_data.get("cells", [])
        edges = self._grid_data.get("edges", [])

        for c in cells:
            r, c_ = int(c["row"]), int(c["col"])
            if c.get("blocked"):
                blocked_set.add((r, c_))
            if "number" in c:
                has_numbers = True
            if "symbol" in c:
                has_symbols = True

        for e in edges:
            if e.get("is_boundary"):
                r1, c1, r2, c2 = e["r1"], e["c1"], e["r2"], e["c2"]
                boundary_set.add((r1, c1, r2, c2))

        # Draw cells
        for r in range(gh):
            for c in range(gw):
                x = ox + c * cell_size
                y = oy + r * cell_size
                rect = QRectF(x, y, cell_size, cell_size)
                if (r, c) in blocked_set:
                    p.fillRect(rect, QColor("#2C3E50"))
                else:
                    p.fillRect(rect, QColor("#F0F4F8"))
                p.setPen(QPen(QColor("#D0D5DD"), 0.5))
                p.drawRect(rect)

        # Draw boundaries
        if boundary_set:
            p.setPen(QPen(QColor("#B8860B"), max(1.5, cell_size * 0.12)))
        for e in edges:
            if e.get("is_boundary"):
                r1, c1, r2, c2 = e["r1"], e["c1"], e["r2"], e["c2"]
                if r1 == r2:
                    y = oy + r1 * cell_size
                    x1 = ox + c1 * cell_size if c1 < c2 else ox + c2 * cell_size
                    x2 = ox + (c1 if c1 > c2 else c2) * cell_size
                else:
                    x = ox + c1 * cell_size
                    y1 = oy + r1 * cell_size if r1 < r2 else oy + r2 * cell_size
                    y2 = oy + (r1 if r1 > r2 else r2) * cell_size
                p.drawLine(QPointF(x1, y), QPointF(x2, y))

                # Draw outer boundaries too
                # (simplified - just show internal boundaries)

        # Summary overlay
        font = QFont("Segoe UI", 8)
        p.setFont(font)
        p.setPen(QColor("#64748B"))
        summary = f"{gh}×{gw}"
        if info.blocked_count:
            summary += f"  {info.blocked_count}障碍"
        if has_numbers:
            summary += " #"
        if has_symbols:
            summary += " ◎"
        p.drawText(QRectF(ox, oy + total_h + 2, total_w, 16),
                   Qt.AlignmentFlag.AlignCenter, summary)

        p.end()


class PuzzleBrowser(QWidget):
    puzzle_selected = Signal(str)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._all_puzzles: list[PuzzleInfo] = []
        self._grid_cache: dict[str, dict] = {}
        self._setup_ui()
        self._scan_puzzles()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(6, 6, 6, 6)
        layout.setSpacing(4)

        # Search bar
        search_layout = QHBoxLayout()
        self._search_input = QLineEdit()
        self._search_input.setPlaceholderText("搜索谜题...")
        self._search_input.setClearButtonEnabled(True)
        self._search_input.textChanged.connect(self._apply_filters)
        self._search_input.setStyleSheet(
            "QLineEdit { padding: 4px 8px; border: 1px solid #D0D5DD; "
            "border-radius: 4px; font-size: 12px; }"
        )
        search_layout.addWidget(self._search_input)

        self._category_combo = QComboBox()
        self._category_combo.addItem("全部")
        self._category_combo.currentIndexChanged.connect(self._apply_filters)
        self._category_combo.setStyleSheet(
            "QComboBox { padding: 4px 6px; border: 1px solid #D0D5DD; "
            "border-radius: 4px; font-size: 12px; }"
        )
        search_layout.addWidget(self._category_combo)
        layout.addLayout(search_layout)

        # List + Preview splitter
        splitter = QSplitter(Qt.Orientation.Vertical)

        self._list_widget = QListWidget()
        self._list_widget.setSpacing(1)
        self._list_widget.currentItemChanged.connect(self._on_selection_changed)
        self._list_widget.itemDoubleClicked.connect(self._on_item_activated)
        self._list_widget.setStyleSheet(
            "QListWidget { border: 1px solid #E0E3E8; border-radius: 4px; "
            "background: #FFFFFF; font-size: 12px; }"
            "QListWidget::item { padding: 4px 6px; }"
            "QListWidget::item:selected { background: #E0F2FE; color: #1E293B; }"
            "QListWidget::item:hover { background: #F1F5F9; }"
        )
        splitter.addWidget(self._list_widget)

        # Preview area
        preview_container = QWidget()
        preview_layout = QVBoxLayout(preview_container)
        preview_layout.setContentsMargins(0, 0, 0, 0)
        preview_layout.setSpacing(2)

        self._preview_widget = PuzzlePreviewWidget()
        preview_layout.addWidget(self._preview_widget)

        self._preview_label = QLabel("选择谜题以预览")
        self._preview_label.setWordWrap(True)
        self._preview_label.setStyleSheet("font-size: 11px; color: #94A3B8; padding: 2px 4px;")
        preview_layout.addWidget(self._preview_label)

        splitter.addWidget(preview_container)
        splitter.setStretchFactor(0, 2)
        splitter.setStretchFactor(1, 1)

        layout.addWidget(splitter, 1)

    def _scan_puzzles(self) -> None:
        self._all_puzzles.clear()
        self._grid_cache.clear()

        seen_categories: set[str] = set()
        category_dirs: list[tuple[str, str]] = []

        if os.path.isdir(PUZZLE_BASE):
            for root, dirs, files in os.walk(PUZZLE_BASE):
                if not any(f.endswith(".json") for f in files):
                    continue
                rel = os.path.relpath(root, PUZZLE_BASE).replace("\\", "/")
                label = rel if rel != "." else os.path.basename(root)
                if label not in seen_categories:
                    seen_categories.add(label)
                    category_dirs.append((label, root))

        category_dirs.sort(key=lambda x: x[0])

        current_categories = [self._category_combo.itemText(i)
                              for i in range(1, self._category_combo.count())]
        new_categories = [c for c, _ in category_dirs]
        if current_categories != new_categories:
            self._category_combo.blockSignals(True)
            while self._category_combo.count() > 1:
                self._category_combo.removeItem(1)
            for label, _ in category_dirs:
                self._category_combo.addItem(label)
            self._category_combo.blockSignals(False)

        for label, directory in category_dirs:
            for fpath in sorted(glob.glob(os.path.join(directory, "*.json"))):
                try:
                    name = os.path.splitext(os.path.basename(fpath))[0]
                    with open(fpath, encoding="utf-8") as f:
                        data = json.load(f)
                    grid = data.get("grid", {})
                    h, w = int(grid.get("height", 0)), int(grid.get("width", 0))
                    rules = [r["type"] for r in data.get("rules", [])]
                    cells = data.get("cells", [])
                    blocked = sum(1 for c in cells if c.get("blocked"))
                    edges = data.get("edges", [])
                    has_bd = any(e.get("is_boundary") for e in edges)

                    info = PuzzleInfo(
                        name=name, path=fpath, category=label,
                        height=h, width=w, rules=rules,
                        blocked_count=blocked, has_boundaries=has_bd,
                    )
                    self._all_puzzles.append(info)
                    self._grid_cache[name] = data
                except Exception:
                    pass

        self._apply_filters()

    def _apply_filters(self) -> None:
        search_text = self._search_input.text().strip().lower()
        category = self._category_combo.currentText()

        self._list_widget.clear()
        for info in self._all_puzzles:
            if category != "全部" and info.category != category:
                continue
            if search_text:
                if search_text not in info.name.lower():
                    if not any(search_text in r.lower() for r in info.rules):
                        continue
            item = QListWidgetItem(f"{info.name}  ({info.height}×{info.width})")
            item.setData(Qt.ItemDataRole.UserRole, info.name)
            self._list_widget.addItem(item)

        self._preview_widget.clear()
        self._preview_label.setText("选择谜题以预览")

    def _on_selection_changed(self, current: QListWidgetItem | None,
                              previous: QListWidgetItem | None) -> None:
        if current is None:
            self._preview_widget.clear()
            self._preview_label.setText("选择谜题以预览")
            return

        name = current.data(Qt.ItemDataRole.UserRole)
        info = next((p for p in self._all_puzzles if p.name == name), None)
        if info is None:
            return

        data = self._grid_cache.get(name)
        if data is not None:
            self._preview_widget.set_puzzle(info, data)

        rules_text = info.rule_display()
        self._preview_label.setText(
            f"<b>{info.name}</b>  {info.height}×{info.width}"
            f"{'  🚫' + str(info.blocked_count) if info.blocked_count else ''}"
            f"<br>{rules_text}"
        )

    def _on_item_activated(self, item: QListWidgetItem) -> None:
        name = item.data(Qt.ItemDataRole.UserRole)
        info = next((p for p in self._all_puzzles if p.name == name), None)
        if info is not None:
            self.puzzle_selected.emit(info.path)

    def refresh(self) -> None:
        self._scan_puzzles()
