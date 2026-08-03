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
    QWidget, QVBoxLayout, QHBoxLayout, QGridLayout, QLineEdit, QComboBox,
    QTreeWidget, QTreeWidgetItem, QLabel, QSplitter, QFrame,
)

from src.ui import theme as _ui_theme

from src.models.puzzle import RULE_NAMES


PUZZLE_BASE = "puzzles"

ALL_RULES = sorted(RULE_NAMES.keys())


class PuzzleInfo:
    __slots__ = ("name", "path", "category", "height", "width",
                 "rules", "blocked_count", "has_boundaries", "difficulty")

    def __init__(self, name: str, path: str, category: str,
                 height: int, width: int, rules: list[str],
                 blocked_count: int, has_boundaries: bool,
                 difficulty: int | None = None) -> None:
        self.name = name
        self.path = path
        self.category = category
        self.height = height
        self.width = width
        self.rules = rules
        self.blocked_count = blocked_count
        self.has_boundaries = has_boundaries
        self.difficulty = difficulty

    @property
    def area(self) -> int:
        return self.height * self.width

    def rule_display(self) -> str:
        return ", ".join(RULE_NAMES.get(r, r) for r in self.rules) if self.rules else "无规则"


def _rule_type_matches(rule_type: str, token: str) -> bool:
    """Match a rule type against a user token (rule key or Chinese name)."""
    token = token.strip().lower()
    if not token:
        return False
    if token in rule_type.lower():
        return True
    name = RULE_NAMES.get(rule_type, "")
    return token in name.lower()


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
        p.fillRect(self.rect(), QColor(_ui_theme.colors.preview_bg))

        info = self._info
        gw = info.width
        gh = info.height
        if gw <= 0 or gh <= 0:
            return

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
        cell_symbols: dict[tuple[int, int], str] = {}
        cell_numbers: dict[tuple[int, int], int] = {}
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
                cell_numbers[(r, c_)] = c["number"]
            if "symbol" in c:
                has_symbols = True
                cell_symbols[(r, c_)] = c["symbol"]

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
                    p.fillRect(rect, QColor(_ui_theme.colors.preview_blocked_bg))
                else:
                    p.fillRect(rect, QColor(_ui_theme.colors.preview_cell_normal))
                p.setPen(QPen(QColor(_ui_theme.colors.preview_cell_border), 0.5))
                p.drawRect(rect)

        # Draw boundaries
        if boundary_set:
            p.setPen(QPen(QColor(_ui_theme.colors.preview_boundary), max(1.5, cell_size * 0.12)))
        for e in edges:
            if e.get("is_boundary"):
                r1, c1, r2, c2 = e["r1"], e["c1"], e["r2"], e["c2"]
                if r1 == r2:
                    x = ox + max(c1, c2) * cell_size
                    y1 = oy + r1 * cell_size
                    y2 = oy + (r1 + 1) * cell_size
                    p.drawLine(QPointF(x, y1), QPointF(x, y2))
                else:
                    y = oy + max(r1, r2) * cell_size
                    x1 = ox + c1 * cell_size
                    x2 = ox + (c1 + 1) * cell_size
                    p.drawLine(QPointF(x1, y), QPointF(x2, y))

        # Draw numbers and symbols
        small_font = QFont("Segoe UI", max(5, cell_size // 2), QFont.Weight.Bold)
        p.setFont(small_font)
        for (r, c_), num in cell_numbers.items():
            x = ox + c_ * cell_size
            y = oy + r * cell_size
            rect = QRectF(x, y, cell_size, cell_size)
            p.setPen(QColor(_ui_theme.colors.number_text))
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter, str(num))
        for (r, c_), sym in cell_symbols.items():
            x = ox + c_ * cell_size
            y = oy + r * cell_size
            rect = QRectF(x, y, cell_size, cell_size)
            p.setPen(QColor(_ui_theme.colors.symbol_text))
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter, sym)

        # Summary overlay
        font = QFont("Segoe UI", 8)
        p.setFont(font)
        p.setPen(QColor(_ui_theme.colors.preview_summary_text))
        summary = f"{gh}×{gw}"
        if info.blocked_count:
            summary += f"  {info.blocked_count}障碍"
        if has_numbers:
            summary += " #"
        if has_symbols:
            summary += " 符"
        p.drawText(QRectF(ox, oy + total_h + 2, total_w, 16),
                   Qt.AlignmentFlag.AlignCenter, summary)

        p.end()


class PuzzleBrowser(QWidget):
    puzzle_selected = Signal(str)

    MODE_ALL = "包含全部 (与)"
    MODE_ANY = "包含任一 (或)"
    MODE_NONE = "排除 (非)"

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

        # ── Filter row 1: search + directory ──────────────────────────
        search_layout = QHBoxLayout()
        self._search_input = QLineEdit()
        self._search_input.setPlaceholderText("搜索名称/规则...")
        self._search_input.setClearButtonEnabled(True)
        self._search_input.textChanged.connect(self._apply_filters)
        self._search_input.setStyleSheet(
            "QLineEdit { padding: 4px 8px; border-radius: 4px; font-size: 12px; }"
        )
        search_layout.addWidget(self._search_input)

        self._category_combo = QComboBox()
        self._category_combo.addItem("全部目录")
        self._category_combo.currentIndexChanged.connect(self._apply_filters)
        self._category_combo.setStyleSheet(
            "QComboBox { padding: 4px 6px; border-radius: 4px; font-size: 12px; }"
        )
        search_layout.addWidget(self._category_combo)
        layout.addLayout(search_layout)

        # ── Filter row 2: rule input + mode + size ────────────────────
        rule_layout = QHBoxLayout()
        self._rule_input = QLineEdit()
        self._rule_input.setPlaceholderText("规则，如: shape_pool, 围栏")
        self._rule_input.textChanged.connect(self._apply_filters)
        self._rule_input.setStyleSheet(
            "QLineEdit { padding: 4px 8px; border-radius: 4px; font-size: 12px; }"
        )
        rule_layout.addWidget(self._rule_input, 1)

        self._rule_mode_combo = QComboBox()
        for mode in (self.MODE_ALL, self.MODE_ANY, self.MODE_NONE):
            self._rule_mode_combo.addItem(mode)
        self._rule_mode_combo.currentIndexChanged.connect(self._apply_filters)
        self._rule_mode_combo.setStyleSheet(
            "QComboBox { padding: 4px 6px; border-radius: 4px; font-size: 12px; }"
        )
        rule_layout.addWidget(self._rule_mode_combo)
        layout.addLayout(rule_layout)

        # ── Filter row 3: size + blocked + boundary + difficulty ──────
        extra_layout = QGridLayout()
        extra_layout.setSpacing(4)

        self._size_combo = QComboBox()
        for text in ("全部大小", "小 (≤25格)", "中 (26~64格)", "大 (>64格)"):
            self._size_combo.addItem(text)
        self._size_combo.currentIndexChanged.connect(self._apply_filters)
        extra_layout.addWidget(QLabel("大小"), 0, 0)
        extra_layout.addWidget(self._size_combo, 0, 1)

        self._blocked_combo = QComboBox()
        for text in ("全部障碍", "有障碍格", "无障碍格"):
            self._blocked_combo.addItem(text)
        self._blocked_combo.currentIndexChanged.connect(self._apply_filters)
        extra_layout.addWidget(QLabel("障碍"), 0, 2)
        extra_layout.addWidget(self._blocked_combo, 0, 3)

        self._boundary_combo = QComboBox()
        for text in ("全部边界", "有预画边界", "无预画边界"):
            self._boundary_combo.addItem(text)
        self._boundary_combo.currentIndexChanged.connect(self._apply_filters)
        extra_layout.addWidget(QLabel("边界"), 0, 4)
        extra_layout.addWidget(self._boundary_combo, 0, 5)

        self._difficulty_combo = QComboBox()
        self._difficulty_combo.addItem("全部难度")
        for d in range(1, 6):
            self._difficulty_combo.addItem(f"难度 {d}")
        self._difficulty_combo.addItem("难度 6+")
        self._difficulty_combo.currentIndexChanged.connect(self._apply_filters)
        extra_layout.addWidget(QLabel("难度"), 1, 0)
        extra_layout.addWidget(self._difficulty_combo, 1, 1)

        extra_layout.setColumnStretch(1, 1)
        extra_layout.setColumnStretch(3, 1)
        extra_layout.setColumnStretch(5, 1)
        layout.addLayout(extra_layout)

        # ── Tree + Preview splitter ───────────────────────────────────
        splitter = QSplitter(Qt.Orientation.Vertical)

        self._tree = QTreeWidget()
        self._tree.setHeaderLabels(["题目"])
        self._tree.setRootIsDecorated(True)
        self._tree.setIndentation(14)
        self._tree.currentItemChanged.connect(self._on_selection_changed)
        self._tree.itemDoubleClicked.connect(self._on_item_activated)
        self._tree.setStyleSheet(
            "QTreeWidget { border-radius: 4px; font-size: 12px; }"
            "QTreeWidget::item { padding: 3px 6px; }"
            "QTreeWidget::item:selected { background: #2A4A6A; }"
            "QTreeWidget::item:hover { background: palette(alternate-base); }"
        )
        splitter.addWidget(self._tree)

        preview_container = QWidget()
        preview_layout = QVBoxLayout(preview_container)
        preview_layout.setContentsMargins(0, 0, 0, 0)
        preview_layout.setSpacing(2)

        self._preview_widget = PuzzlePreviewWidget()
        preview_layout.addWidget(self._preview_widget)

        self._preview_label = QLabel("选择题目以预览")
        self._preview_label.setWordWrap(True)
        self._preview_label.setStyleSheet("font-size: 11px; padding: 2px 4px;")
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
                    meta = data.get("_meta", {})
                    difficulty = meta.get("archive_difficulty")

                    info = PuzzleInfo(
                        name=name, path=fpath, category=label,
                        height=h, width=w, rules=rules,
                        blocked_count=blocked, has_boundaries=has_bd,
                        difficulty=difficulty,
                    )
                    self._all_puzzles.append(info)
                    self._grid_cache[name] = data
                except Exception:
                    pass

        self._apply_filters()

    # ── Filter evaluation ────────────────────────────────────────────

    def _parse_rule_tokens(self) -> list[str]:
        raw = self._rule_input.text()
        return [t.strip() for t in raw.replace("，", ",").split(",") if t.strip()]

    def _matches_rules(self, info: PuzzleInfo) -> bool:
        tokens = self._parse_rule_tokens()
        if not tokens:
            return True
        # expand each token to the rule types it matches
        matched: set[str] = set()
        for tok in tokens:
            for r in info.rules:
                if _rule_type_matches(r, tok):
                    matched.add(r)
        mode = self._rule_mode_combo.currentText()
        if mode == self.MODE_ALL:
            # every token must match at least one rule
            for tok in tokens:
                if not any(_rule_type_matches(r, tok) for r in info.rules):
                    return False
            return True
        if mode == self.MODE_ANY:
            return bool(matched)
        # MODE_NONE: exclude puzzles having any listed rule
        return not matched

    def _matches_size(self, info: PuzzleInfo) -> bool:
        text = self._size_combo.currentText()
        area = info.area
        if text.startswith("小"):
            return area <= 25
        if text.startswith("中"):
            return 26 <= area <= 64
        if text.startswith("大"):
            return area > 64
        return True

    def _matches_extra(self, info: PuzzleInfo) -> bool:
        blk = self._blocked_combo.currentText()
        if blk == "有障碍格" and info.blocked_count == 0:
            return False
        if blk == "无障碍格" and info.blocked_count > 0:
            return False

        bnd = self._boundary_combo.currentText()
        if bnd == "有预画边界" and not info.has_boundaries:
            return False
        if bnd == "无预画边界" and info.has_boundaries:
            return False

        diff = self._difficulty_combo.currentText()
        if diff.startswith("难度"):
            try:
                want = int(diff.split()[1].rstrip("+"))
            except Exception:
                want = None
            if want is not None:
                d = info.difficulty if info.difficulty is not None else 0
                if diff.endswith("+") and d < want:
                    return False
                if not diff.endswith("+") and d != want:
                    return False
        return True

    def _apply_filters(self) -> None:
        search_text = self._search_input.text().strip().lower()
        category = self._category_combo.currentText()

        self._tree.clear()
        grouped: dict[str, list[PuzzleInfo]] = {}
        for info in self._all_puzzles:
            if category != "全部目录" and info.category != category:
                continue
            if not self._matches_rules(info):
                continue
            if not self._matches_size(info):
                continue
            if not self._matches_extra(info):
                continue
            if search_text:
                if search_text not in info.name.lower():
                    if not any(search_text in r.lower() for r in info.rules):
                        if not any(search_text in RULE_NAMES.get(r, r).lower() for r in info.rules):
                            continue
            grouped.setdefault(info.category, []).append(info)

        total = 0
        for cat in sorted(grouped):
            top = QTreeWidgetItem([f"{cat}  ({len(grouped[cat])})"])
            top.setData(0, Qt.ItemDataRole.UserRole, None)
            for info in grouped[cat]:
                child = QTreeWidgetItem([f"{info.name}  ({info.height}×{info.width})"])
                child.setData(0, Qt.ItemDataRole.UserRole, info.name)
                top.addChild(child)
            self._tree.addTopLevelItem(top)
            total += len(grouped[cat])

        self._tree.expandAll()
        self._tree.setHeaderLabel(f"题目 ({total})")
        self._preview_widget.clear()
        self._preview_label.setText("选择题目以预览")

    def _find_info(self, name: str) -> PuzzleInfo | None:
        return next((p for p in self._all_puzzles if p.name == name), None)

    def _on_selection_changed(self, current: QTreeWidgetItem | None,
                              previous: QTreeWidgetItem | None) -> None:
        if current is None:
            self._preview_widget.clear()
            self._preview_label.setText("选择题目以预览")
            return
        name = current.data(0, Qt.ItemDataRole.UserRole)
        if not name:
            self._preview_widget.clear()
            self._preview_label.setText("选择题目以预览")
            return
        info = self._find_info(name)
        if info is None:
            return

        data = self._grid_cache.get(name)
        if data is not None:
            self._preview_widget.set_puzzle(info, data)

        rules_text = info.rule_display()
        difficulty = f"  难度{info.difficulty}" if info.difficulty is not None else ""
        self._preview_label.setText(
            f"<b>{info.name}</b>  {info.height}×{info.width}{difficulty}"
            f"{'  ' + str(info.blocked_count) + '障碍' if info.blocked_count else ''}"
            f"<br>{rules_text}"
        )

    def _on_item_activated(self, item: QTreeWidgetItem, column: int) -> None:
        name = item.data(0, Qt.ItemDataRole.UserRole)
        if not name:
            return
        info = self._find_info(name)
        if info is not None:
            self.puzzle_selected.emit(info.path)

    def refresh(self) -> None:
        self._scan_puzzles()
