from __future__ import annotations

from typing import Optional, Callable

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QCheckBox, QLabel,
    QSpinBox, QPushButton, QScrollArea, QGroupBox, QFormLayout,
    QListWidget, QListWidgetItem, QFrame,
)

from src.models.board import Shape
from src.models.puzzle import Puzzle, Rule, RULE_NAMES
from src.ui.shape_editor import ShapeEditorDialog
from src.ui.theme import RULE_CATEGORIES


RULE_DESCRIPTIONS: dict[str, str] = {
    "shape_pool": "区域形状必须来自形状池",
    "rose_window": "N种符号各M个，M个区域各含全部N种",
    "heterogeneous": "标记边两侧区域形状不同",
    "homogeneous": "标记边两侧区域形状相同",
    "precise": "所有区域面积=指定值",
    "puzzle_piece": "标记格的区域形状=标记图案",
    "mixed": "相邻区域形状互不相同",
    "area": "标记格的区域面积=数字",
    "same": "所有区域形状相同",
    "range": "区域面积在[min,max]内",
    "fence": "边界分布匹配标记图案",
    "different": "所有区域形状互不相同",
    "solitary": "每区域仅含一个符号",
    "block": "所有区域为矩形",
    "non_block": "所有区域非矩形",
    "differentiation": "相邻区域面积不等",
    "brick": "禁止四边同交于一点",
    "ring": "禁止三边同交于一点",
    "inequality": "不等号指向面积更小侧（> 左大 · < 右大 · ^ 上大 · v 下大）",
    "difference": "边数字=两侧面积差",
    "watchtower": "顶点数字=相邻区域数",
    "compass": "四方向同区域格数",
}


class ConstraintPanel(QWidget):
    rules_changed = Signal()

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._puzzle: Puzzle | None = None
        self._checkboxes: dict[str, QCheckBox] = {}
        self._param_widgets: dict[str, QWidget] = {}
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(4)

        title = QLabel("规则配置")
        title.setStyleSheet("font-size: 14px; font-weight: bold; padding: 2px 0;")
        layout.addWidget(title)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setStyleSheet("QScrollArea { border: none; background: transparent; }")

        container = QWidget()
        self._rules_layout = QVBoxLayout(container)
        self._rules_layout.setContentsMargins(0, 0, 0, 0)
        self._rules_layout.setSpacing(8)

        for category_name, rule_types in RULE_CATEGORIES:
            self._add_category_group(category_name, rule_types)

        scroll.setWidget(container)
        layout.addWidget(scroll, 1)

        btn_layout = QHBoxLayout()
        clear_btn = QPushButton("全部清除")
        clear_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        clear_btn.setStyleSheet(
            "QPushButton { font-size: 12px; padding: 6px 16px; }"
        )
        clear_btn.clicked.connect(self._clear_all)
        btn_layout.addStretch()
        btn_layout.addWidget(clear_btn)
        layout.addLayout(btn_layout)

    def _add_category_group(self, category_name: str, rule_types: list[str]) -> None:
        group = QGroupBox(category_name)
        group.setStyleSheet("""
            QGroupBox {
                font-size: 12px;
                font-weight: bold;
                border: 1px solid palette(mid);
                border-radius: 6px;
                margin-top: 8px;
                padding: 12px 6px 6px;
            }
            QGroupBox::title {
                subcontrol-origin: margin;
                subcontrol-position: top left;
                padding: 0 8px;
                left: 8px;
            }
        """)
        vbox = QVBoxLayout(group)
        vbox.setContentsMargins(4, 8, 4, 4)
        vbox.setSpacing(2)

        for rule_type in rule_types:
            self._add_rule_row(vbox, rule_type)

        self._rules_layout.addWidget(group)

    def _add_rule_row(self, parent_layout: QVBoxLayout, rule_type: str) -> None:
        display_name = RULE_NAMES.get(rule_type, rule_type)

        row = QVBoxLayout()
        row.setContentsMargins(4, 2, 4, 2)
        row.setSpacing(1)

        header = QHBoxLayout()
        cb = QCheckBox(display_name)
        cb.toggled.connect(lambda checked, t=rule_type: self._on_rule_toggled(t, checked))
        self._checkboxes[rule_type] = cb
        header.addWidget(cb)
        header.addStretch()
        row.addLayout(header)

        desc = QLabel(RULE_DESCRIPTIONS.get(rule_type, ""))
        desc.setStyleSheet("font-size: 11px; padding-left: 24px;")
        desc.setWordWrap(True)
        row.addWidget(desc)

        params_widget = self._create_params_widget(rule_type)
        if params_widget is not None:
            params_widget.setVisible(False)
            self._param_widgets[rule_type] = params_widget
            row.addWidget(params_widget)

        parent_layout.addLayout(row)

    def _create_params_widget(self, rule_type: str) -> QWidget | None:
        w = QWidget()
        layout = QHBoxLayout(w)
        layout.setContentsMargins(24, 2, 4, 2)
        layout.setSpacing(6)

        if rule_type == "precise":
            layout.addWidget(QLabel("面积:"))
            spin = QSpinBox()
            spin.setRange(1, 256)
            # Sensible default so toggling the rule ON without editing the spin
            # yields a solvable constraint (bug C3: an empty params dict would
            # resolve `area` to 0, making every region illegal).
            spin.setValue(1)
            spin.valueChanged.connect(lambda v, t=rule_type: self._on_param_changed(t, "area", v))
            self._param_spin(rule_type, "area", spin)
            layout.addWidget(spin)
            layout.addStretch()

        elif rule_type == "range":
            layout.addWidget(QLabel("最小:"))
            min_spin = QSpinBox()
            min_spin.setRange(1, 256)
            min_spin.setValue(1)
            min_spin.valueChanged.connect(lambda v, t=rule_type: self._on_param_changed(t, "min", v))
            self._param_spin(rule_type, "min", min_spin)
            layout.addWidget(min_spin)
            layout.addWidget(QLabel("最大:"))
            max_spin = QSpinBox()
            max_spin.setRange(1, 256)
            max_spin.setValue(256)
            max_spin.valueChanged.connect(lambda v, t=rule_type: self._on_param_changed(t, "max", v))
            self._param_spin(rule_type, "max", max_spin)
            layout.addWidget(max_spin)
            layout.addStretch()

        elif rule_type == "shape_pool":
            edit_btn = QPushButton("编辑形状池...")
            edit_btn.setCursor(Qt.CursorShape.PointingHandCursor)
            edit_btn.clicked.connect(self._open_shape_editor)
            layout.addWidget(edit_btn)
            self._shape_count_label = QLabel("0 个形状")
            layout.addWidget(self._shape_count_label)
            layout.addStretch()

        elif rule_type == "rose_window":
            self._rose_input = QPushButton("配置符号...")
            self._rose_input.setFixedWidth(100)
            self._rose_input.setCursor(Qt.CursorShape.PointingHandCursor)
            layout.addWidget(self._rose_input)
            layout.addStretch()

        else:
            return None

        return w

    def _open_shape_editor(self) -> None:
        existing = []
        if self._puzzle is not None:
            rule = self._puzzle.get_rule("shape_pool")
            if rule is not None:
                existing = rule.params.get("shapes", [])
        dialog = ShapeEditorDialog(self, existing_shapes=existing)
        if dialog.exec():
            shapes = dialog.get_shapes()
            if self._puzzle is not None:
                rule = self._puzzle.get_rule("shape_pool")
                if rule is not None:
                    rule.params["shapes"] = shapes
                else:
                    self._puzzle.rules.append(Rule.shape_pool(shapes))
            self._update_shape_count(len(shapes))
            self.rules_changed.emit()

    def _update_shape_count(self, n: int) -> None:
        if hasattr(self, "_shape_count_label"):
            self._shape_count_label.setText(f"{n} 个形状")

    def _param_spin(self, rule_type: str, param_name: str, spin: QSpinBox) -> None:
        if not hasattr(self, "_param_spins"):
            self._param_spins: dict[str, dict[str, QSpinBox]] = {}
        if rule_type not in self._param_spins:
            self._param_spins[rule_type] = {}
        self._param_spins[rule_type][param_name] = spin

    def _on_rule_toggled(self, rule_type: str, checked: bool) -> None:
        if rule_type in self._param_widgets:
            self._param_widgets[rule_type].setVisible(checked)
        if self._puzzle is not None:
            if checked:
                # Populate default params from the spin widgets so a freshly
                # toggled rule is solvable instead of carrying an empty params
                # dict (bug C3: empty params → `area`/`min` default to 0).
                self._puzzle.rules.append(
                    Rule(type=rule_type, params=self._collect_params(rule_type))
                )
            else:
                self._puzzle.rules = [r for r in self._puzzle.rules if r.type != rule_type]
        self.rules_changed.emit()

    def _collect_params(self, rule_type: str) -> dict:
        """Read the current param-spin values for a rule type.

        Used when a rule is toggled ON so its params are seeded from the UI
        defaults rather than left empty (bug C3).  Rules without param spins
        (e.g. shape_pool, rose_window) return an empty dict.
        """
        params: dict = {}
        if hasattr(self, "_param_spins"):
            for pname, spin in self._param_spins.get(rule_type, {}).items():
                params[pname] = spin.value()
        return params

    def _on_param_changed(self, rule_type: str, param: str, value: int) -> None:
        if self._puzzle is None:
            return
        rule = self._puzzle.get_rule(rule_type)
        if rule is not None:
            rule.params[param] = value

    def _clear_all(self) -> None:
        for cb in self._checkboxes.values():
            cb.setChecked(False)
        if self._puzzle is not None:
            self._puzzle.rules.clear()
        self.rules_changed.emit()
        self._update_shape_count(0)

    def set_puzzle(self, puzzle: Puzzle) -> None:
        self._puzzle = puzzle
        for rule_type, cb in self._checkboxes.items():
            has_rule = puzzle.has_rule(rule_type)
            # Block the signal: `setChecked(True)` would otherwise fire
            # `toggled(True)` → `_on_rule_toggled` appends the rule AGAIN,
            # duplicating every rule already present in the loaded puzzle
            # (bug C1: [area,block] → [area,block,block,area]).
            cb.blockSignals(True)
            cb.setChecked(has_rule)
            cb.blockSignals(False)
            if rule_type in self._param_widgets:
                self._param_widgets[rule_type].setVisible(has_rule)
            if has_rule and hasattr(self, "_param_spins"):
                rule = puzzle.get_rule(rule_type)
                if rule is not None:
                    spins = self._param_spins.get(rule_type, {})
                    for pname, spin in spins.items():
                        if pname in rule.params:
                            spin.setValue(rule.params[pname])

        shape_rule = puzzle.get_rule("shape_pool")
        if shape_rule is not None:
            shapes = shape_rule.params.get("shapes", [])
            self._update_shape_count(len(shapes))
