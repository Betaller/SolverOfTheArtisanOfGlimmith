from __future__ import annotations

import glob
import json
import logging
import os
from typing import Any, NamedTuple

from PySide6.QtCore import QPointF, QRectF, Qt, Signal
from PySide6.QtGui import QColor, QFont, QPainter, QPaintEvent, QPen
from PySide6.QtWidgets import (
    QComboBox,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QSplitter,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
    QWidget,
)

from src.models.puzzle import RULE_NAMES
from src.ui import theme as _ui_theme

logger = logging.getLogger(__name__)

PUZZLE_BASE = "puzzles"

ALL_RULES = sorted(RULE_NAMES.keys())


class PuzzleInfo:
    __slots__ = (
        "name",
        "path",
        "category",
        "height",
        "width",
        "rules",
        "blocked_count",
        "has_boundaries",
        "difficulty",
    )

    def __init__(
        self,
        name: str,
        path: str,
        category: str,
        height: int,
        width: int,
        rules: list[str],
        blocked_count: int,
        has_boundaries: bool,
        difficulty: int | None = None,
    ) -> None:
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


class _PreviewGeom(NamedTuple):
    """Layout of the preview grid inside the widget."""

    ox: float
    oy: float
    cell_size: int
    total_w: int
    total_h: int


def _preview_geom(view_w: int, view_h: int, gw: int, gh: int) -> _PreviewGeom | None:
    if gw <= 0 or gh <= 0:
        return None
    margin = 8
    avail_w = view_w - margin * 2
    avail_h = view_h - margin * 2
    cell_size = max(4, min(avail_w // max(gw, 1), avail_h // max(gh, 1)))
    cell_size = min(cell_size, 14)
    total_w = gw * cell_size
    total_h = gh * cell_size
    ox = margin + (avail_w - total_w) / 2
    oy = margin + (avail_h - total_h) / 2
    return _PreviewGeom(ox, oy, cell_size, total_w, total_h)


def _blocked_cells(cells: list[dict[str, Any]]) -> set[tuple[int, int]]:
    return {(int(c["row"]), int(c["col"])) for c in cells if c.get("blocked")}


def _cell_numbers(cells: list[dict[str, Any]]) -> dict[tuple[int, int], Any]:
    return {(int(c["row"]), int(c["col"])): c["number"] for c in cells if "number" in c}


def _cell_symbols(cells: list[dict[str, Any]]) -> dict[tuple[int, int], Any]:
    return {(int(c["row"]), int(c["col"])): c["symbol"] for c in cells if "symbol" in c}


def _draw_cells(
    p: QPainter, geom: _PreviewGeom, gw: int, gh: int, blocked: set[tuple[int, int]]
) -> None:
    border = QPen(QColor(_ui_theme.colors.preview_cell_border), 0.5)
    normal = QColor(_ui_theme.colors.preview_cell_normal)
    blocked_bg = QColor(_ui_theme.colors.preview_blocked_bg)
    for r in range(gh):
        for c in range(gw):
            rect = QRectF(
                geom.ox + c * geom.cell_size,
                geom.oy + r * geom.cell_size,
                geom.cell_size,
                geom.cell_size,
            )
            p.fillRect(rect, blocked_bg if (r, c) in blocked else normal)
            p.setPen(border)
            p.drawRect(rect)


def _draw_boundaries(p: QPainter, geom: _PreviewGeom, edges: list[dict[str, Any]]) -> None:
    drawn = [e for e in edges if e.get("is_boundary")]
    if not drawn:
        return
    p.setPen(QPen(QColor(_ui_theme.colors.preview_boundary), max(1.5, geom.cell_size * 0.12)))
    for e in drawn:
        r1, c1, r2, c2 = e["r1"], e["c1"], e["r2"], e["c2"]
        if r1 == r2:
            x = geom.ox + max(c1, c2) * geom.cell_size
            y1 = geom.oy + r1 * geom.cell_size
            y2 = geom.oy + (r1 + 1) * geom.cell_size
            p.drawLine(QPointF(x, y1), QPointF(x, y2))
        else:
            y = geom.oy + max(r1, r2) * geom.cell_size
            x1 = geom.ox + c1 * geom.cell_size
            x2 = geom.ox + (c1 + 1) * geom.cell_size
            p.drawLine(QPointF(x1, y), QPointF(x2, y))


def _draw_annotations(
    p: QPainter,
    geom: _PreviewGeom,
    numbers: dict[tuple[int, int], Any],
    symbols: dict[tuple[int, int], Any],
) -> None:
    p.setFont(QFont("Segoe UI", max(5, geom.cell_size // 2), QFont.Weight.Bold))
    for mapping, color in (
        (numbers, _ui_theme.colors.number_text),
        (symbols, _ui_theme.colors.symbol_text),
    ):
        p.setPen(QColor(color))
        for (r, c), value in mapping.items():
            rect = QRectF(
                geom.ox + c * geom.cell_size,
                geom.oy + r * geom.cell_size,
                geom.cell_size,
                geom.cell_size,
            )
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter, str(value))


def _draw_summary(
    p: QPainter, geom: _PreviewGeom, info: PuzzleInfo, has_numbers: bool, has_symbols: bool
) -> None:
    p.setFont(QFont("Segoe UI", 8))
    p.setPen(QColor(_ui_theme.colors.preview_summary_text))
    summary = f"{info.height}×{info.width}"
    if info.blocked_count:
        summary += f"  {info.blocked_count}障碍"
    if has_numbers:
        summary += " #"
    if has_symbols:
        summary += " 符"
    p.drawText(
        QRectF(geom.ox, geom.oy + geom.total_h + 2, geom.total_w, 16),
        Qt.AlignmentFlag.AlignCenter,
        summary,
    )


class PuzzlePreviewWidget(QWidget):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._info: PuzzleInfo | None = None
        self._grid_data: dict[str, Any] | None = None
        self.setMinimumSize(200, 160)
        self.setMaximumHeight(200)

    def set_puzzle(self, info: PuzzleInfo, grid_data: dict[str, Any]) -> None:
        self._info = info
        self._grid_data = grid_data
        self.update()

    def clear(self) -> None:
        self._info = None
        self._grid_data = None
        self.update()

    def paintEvent(self, event: QPaintEvent) -> None:  # noqa: N802 - Qt override
        super().paintEvent(event)
        if self._info is None or self._grid_data is None:
            return

        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.fillRect(self.rect(), QColor(_ui_theme.colors.preview_bg))

        info = self._info
        geom = _preview_geom(self.width(), self.height(), info.width, info.height)
        if geom is None:
            return

        cells = self._grid_data.get("cells", [])
        edges = self._grid_data.get("edges", [])
        numbers = _cell_numbers(cells)
        symbols = _cell_symbols(cells)

        _draw_cells(p, geom, info.width, info.height, _blocked_cells(cells))
        _draw_boundaries(p, geom, edges)
        _draw_annotations(p, geom, numbers, symbols)
        _draw_summary(p, geom, info, bool(numbers), bool(symbols))
        p.end()


class PuzzleBrowser(QWidget):
    puzzle_selected = Signal(str)

    MODE_ALL = "包含全部 (与)"
    MODE_ANY = "包含任一 (或)"
    MODE_NONE = "排除 (非)"

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._all_puzzles: list[PuzzleInfo] = []
        self._grid_cache: dict[str, dict[str, Any]] = {}
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

    def _collect_category_dirs(self) -> list[tuple[str, str]]:
        """Walk ``PUZZLE_BASE`` and return ``(label, directory)`` pairs."""
        seen: set[str] = set()
        found: list[tuple[str, str]] = []
        if not os.path.isdir(PUZZLE_BASE):
            return found
        for root, _dirs, files in os.walk(PUZZLE_BASE):
            if not any(f.endswith(".json") for f in files):
                continue
            rel = os.path.relpath(root, PUZZLE_BASE).replace("\\", "/")
            # 跳过官方解 answer 目录（`*-answer`），它们不是可解谜题。
            if any(part.endswith("-answer") for part in rel.split("/")):
                continue
            label = rel if rel != "." else os.path.basename(root)
            if label not in seen:
                seen.add(label)
                found.append((label, root))
        return found

    def _sync_category_combo(self, category_dirs: list[tuple[str, str]]) -> None:
        current = [self._category_combo.itemText(i) for i in range(1, self._category_combo.count())]
        if current == [label for label, _ in category_dirs]:
            return
        self._category_combo.blockSignals(True)
        while self._category_combo.count() > 1:
            self._category_combo.removeItem(1)
        for label, _ in category_dirs:
            self._category_combo.addItem(label)
        self._category_combo.blockSignals(False)

    @staticmethod
    def _parse_puzzle(fpath: str, label: str) -> tuple[PuzzleInfo, Any]:
        with open(fpath, encoding="utf-8") as f:
            data = json.load(f)
        grid = data.get("grid", {})
        cells = data.get("cells", [])
        edges = data.get("edges", [])
        info = PuzzleInfo(
            name=os.path.splitext(os.path.basename(fpath))[0],
            path=fpath,
            category=label,
            height=int(grid.get("height", 0)),
            width=int(grid.get("width", 0)),
            rules=[r["type"] for r in data.get("rules", [])],
            blocked_count=sum(1 for c in cells if c.get("blocked")),
            has_boundaries=any(e.get("is_boundary") for e in edges),
            difficulty=data.get("_meta", {}).get("archive_difficulty"),
        )
        return info, data

    def _load_puzzle(self, fpath: str, label: str) -> None:
        try:
            info, data = self._parse_puzzle(fpath, label)
        except Exception as e:
            # Surface the failure instead of silently swallowing it, so a
            # corrupt/malformed puzzle file is at least logged (bug L7).
            logger.warning("解析谜题失败 %s: %s", fpath, e)
            return
        self._all_puzzles.append(info)
        # Key the cache by the FULL file path, not the basename: two
        # puzzles can share a basename in different directories and
        # must not collide (bug L7).
        self._grid_cache[fpath] = data

    def _scan_puzzles(self) -> None:
        self._all_puzzles.clear()
        self._grid_cache.clear()

        category_dirs = self._collect_category_dirs()
        category_dirs.sort(key=lambda x: x[0])
        self._sync_category_combo(category_dirs)

        for label, directory in category_dirs:
            for fpath in sorted(glob.glob(os.path.join(directory, "*.json"))):
                self._load_puzzle(fpath, label)

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
            return all(any(_rule_type_matches(r, tok) for r in info.rules) for tok in tokens)
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

    def _matches_blocked(self, info: PuzzleInfo) -> bool:
        text = self._blocked_combo.currentText()
        if text == "有障碍格":
            return info.blocked_count > 0
        if text == "无障碍格":
            return info.blocked_count == 0
        return True

    def _matches_boundary(self, info: PuzzleInfo) -> bool:
        text = self._boundary_combo.currentText()
        if text == "有预画边界":
            return info.has_boundaries
        if text == "无预画边界":
            return not info.has_boundaries
        return True

    @staticmethod
    def _parse_difficulty(text: str) -> int | None:
        try:
            return int(text.split()[1].rstrip("+"))
        except Exception:
            return None

    def _matches_difficulty(self, info: PuzzleInfo) -> bool:
        text = self._difficulty_combo.currentText()
        if not text.startswith("难度"):
            return True
        want = self._parse_difficulty(text)
        if want is None:
            return True
        actual = info.difficulty if info.difficulty is not None else 0
        return actual >= want if text.endswith("+") else actual == want

    def _matches_extra(self, info: PuzzleInfo) -> bool:
        return (
            self._matches_blocked(info)
            and self._matches_boundary(info)
            and self._matches_difficulty(info)
        )

    def _matches_search(self, info: PuzzleInfo, search_text: str) -> bool:
        if not search_text:
            return True
        if search_text in info.name.lower():
            return True
        if any(search_text in r.lower() for r in info.rules):
            return True
        return any(search_text in RULE_NAMES.get(r, r).lower() for r in info.rules)

    def _accepts(self, info: PuzzleInfo, category: str, search_text: str) -> bool:
        if category != "全部目录" and info.category != category:
            return False
        return (
            self._matches_rules(info)
            and self._matches_size(info)
            and self._matches_extra(info)
            and self._matches_search(info, search_text)
        )

    def _apply_filters(self) -> None:
        search_text = self._search_input.text().strip().lower()
        category = self._category_combo.currentText()

        self._tree.clear()
        grouped: dict[str, list[PuzzleInfo]] = {}
        for info in self._all_puzzles:
            if self._accepts(info, category, search_text):
                grouped.setdefault(info.category, []).append(info)

        self._populate_tree(grouped)

    def _populate_tree(self, grouped: dict[str, list[PuzzleInfo]]) -> None:
        total = 0
        for cat in sorted(grouped):
            top = QTreeWidgetItem([f"{cat}  ({len(grouped[cat])})"])
            top.setData(0, Qt.ItemDataRole.UserRole, None)
            for info in grouped[cat]:
                child = QTreeWidgetItem([f"{info.name}  ({info.height}×{info.width})"])
                # Store the full path (not the basename) so selections stay
                # unambiguous across directories (bug L7).
                child.setData(0, Qt.ItemDataRole.UserRole, info.path)
                top.addChild(child)
            self._tree.addTopLevelItem(top)
            total += len(grouped[cat])

        self._tree.expandAll()
        self._tree.setHeaderLabel(f"题目 ({total})")
        self._preview_widget.clear()
        self._preview_label.setText("选择题目以预览")

    def _find_info(self, path: str) -> PuzzleInfo | None:
        return next((p for p in self._all_puzzles if p.path == path), None)

    def _on_selection_changed(
        self,
        current: QTreeWidgetItem | None,
        previous: QTreeWidgetItem | None,  # noqa: ARG002 - required by Qt signal
    ) -> None:
        if current is None:
            self._preview_widget.clear()
            self._preview_label.setText("选择题目以预览")
            return
        key = current.data(0, Qt.ItemDataRole.UserRole)
        if not key:
            self._preview_widget.clear()
            self._preview_label.setText("选择题目以预览")
            return
        info = self._find_info(key)
        if info is None:
            return

        data = self._grid_cache.get(key)
        if data is not None:
            self._preview_widget.set_puzzle(info, data)

        rules_text = info.rule_display()
        difficulty = f"  难度{info.difficulty}" if info.difficulty is not None else ""
        self._preview_label.setText(
            f"<b>{info.name}</b>  {info.height}×{info.width}{difficulty}"
            f"{'  ' + str(info.blocked_count) + '障碍' if info.blocked_count else ''}"
            f"<br>{rules_text}"
        )

    def _on_item_activated(self, item: QTreeWidgetItem, column: int) -> None:  # noqa: ARG002
        key = item.data(0, Qt.ItemDataRole.UserRole)
        if not key:
            return
        info = self._find_info(key)
        if info is not None:
            self.puzzle_selected.emit(info.path)

    def refresh(self) -> None:
        self._scan_puzzles()
