from __future__ import annotations

from typing import Optional
from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QWidget, QVBoxLayout, QPushButton, QLabel, QLineEdit,
    QHBoxLayout, QButtonGroup, QGridLayout, QGroupBox,
    QSpinBox, QFrame,
)
from PySide6.QtGui import QIntValidator

from src.models.board import CompassClue
from src.ui.theme import MODE_COLORS


MODE_BUTTON_STYLE = """
QPushButton {
    text-align: left;
    padding: 5px 10px;
    border-left: 3px solid %s;
    border-radius: 5px;
    font-size: 12px;
}
QPushButton:checked {
    background: %s;
    color: #FFFFFF;
    border-color: %s;
    border-left: 3px solid %s;
}
"""


class ToolPalette(QWidget):
    mode_changed = Signal(str)
    symbol_changed = Signal(str)
    number_changed = Signal(object)
    grid_size_changed = Signal(int, int)
    compass_applied = Signal(object)
    watchtower_changed = Signal(int)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._current_mode = "select"
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(6, 4, 6, 4)
        layout.setSpacing(3)

        self._setup_tool_buttons(layout)

        self._add_separator(layout)

        self._add_section_header(layout, "盘面大小")
        self._setup_grid_size(layout)

        self._add_separator(layout)

        self._add_section_header(layout, "数字")
        self._setup_number_input(layout)

        self._add_separator(layout)

        self._add_section_header(layout, "望塔值")
        self._setup_watchtower_input(layout)

        self._add_separator(layout)

        self._add_section_header(layout, "符号")
        self._setup_symbol_input(layout)

        self._add_separator(layout)

        self._add_section_header(layout, "罗盘")
        self._setup_compass_input(layout)

        layout.addStretch()

    def _add_section_header(self, layout: QVBoxLayout, text: str) -> None:
        label = QLabel(text)
        label.setStyleSheet(
            "font-size: 10px; font-weight: bold; padding: 1px 0;"
        )
        layout.addWidget(label)

    def _add_separator(self, layout: QVBoxLayout) -> None:
        line = QFrame()
        line.setFrameShape(QFrame.Shape.HLine)
        line.setStyleSheet("background: palette(mid); max-height: 1px; margin: 2px 0;")
        layout.addWidget(line)

    def _make_tool_button(self, mode: str, label: str) -> QPushButton:
        color = MODE_COLORS.get(mode, "#5B9BD5")
        hover_bg = color  # for checked state
        style = MODE_BUTTON_STYLE % (color, hover_bg, color, color)
        btn = QPushButton(label)
        btn.setCheckable(True)
        btn.setStyleSheet(style)
        btn.setCursor(Qt.CursorShape.PointingHandCursor)
        return btn

    def _setup_tool_buttons(self, layout: QVBoxLayout) -> None:
        self._btn_group = QButtonGroup(self)
        self._btn_group.setExclusive(True)

        tools: list[tuple[str, str, str]] = [
            ("select", "\U0001F4CC 选择", "选择/查看单元格属性 (V)"),
            ("boundary", "\u2501 边框绘制", "点击顶点拖拽绘制分割线 (B)"),
            ("block", "\u2716 障碍格", "点击切换障碍格 (X)"),
            ("number", "# 数字标注", "点击输入数字线索 (N)"),
            ("symbol", "\u2605 符号标注", "点击输入符号 (S)"),
            ("compass", "\u25CE 罗盘标注", "点击设置四方向计数 (C)"),
            ("watchtower", "\u25C9 望塔标注", "点击顶点设置望塔值 (W)"),
        ]

        for mode, label, tip in tools:
            btn = self._make_tool_button(mode, label)
            btn.setToolTip(tip)
            if mode == "select":
                btn.setChecked(True)
            btn.clicked.connect(lambda checked, m=mode: self._on_mode_selected(m))
            self._btn_group.addButton(btn)
            layout.addWidget(btn)

    def _setup_grid_size(self, layout: QVBoxLayout) -> None:
        row = QHBoxLayout()
        row.setSpacing(6)

        wbox = QVBoxLayout()
        wbox.setSpacing(0)
        wlabel = QLabel("列")
        wlabel.setStyleSheet("font-size: 9px; padding: 0;")
        wlabel.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._width_spin = QSpinBox()
        self._width_spin.setRange(2, 50)
        self._width_spin.setValue(6)
        self._width_spin.setFixedWidth(80)
        self._width_spin.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._width_spin.valueChanged.connect(self._on_grid_size_changed)
        wbox.addWidget(self._width_spin)
        wbox.addWidget(wlabel)
        row.addLayout(wbox)

        label = QLabel("\u00d7")
        label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        label.setStyleSheet("font-size: 16px; font-weight: bold; padding: 6px 0 0 0;")
        row.addWidget(label)

        hbox = QVBoxLayout()
        hbox.setSpacing(0)
        hlabel = QLabel("行")
        hlabel.setStyleSheet("font-size: 9px; padding: 0;")
        hlabel.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._height_spin = QSpinBox()
        self._height_spin.setRange(2, 50)
        self._height_spin.setValue(6)
        self._height_spin.setFixedWidth(80)
        self._height_spin.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._height_spin.valueChanged.connect(self._on_grid_size_changed)
        hbox.addWidget(self._height_spin)
        hbox.addWidget(hlabel)
        row.addLayout(hbox)

        layout.addLayout(row)

    def _on_grid_size_changed(self) -> None:
        self.grid_size_changed.emit(self._width_spin.value(), self._height_spin.value())

    def set_grid_size(self, width: int, height: int) -> None:
        self._width_spin.blockSignals(True)
        self._height_spin.blockSignals(True)
        self._width_spin.setValue(width)
        self._height_spin.setValue(height)
        self._width_spin.blockSignals(False)
        self._height_spin.blockSignals(False)

    def _setup_number_input(self, layout: QVBoxLayout) -> None:
        self._number_input = QLineEdit()
        self._number_input.setPlaceholderText("数字 (0-999)")
        self._number_input.setValidator(QIntValidator(0, 999))
        self._number_input.textChanged.connect(self._on_number_changed)
        layout.addWidget(self._number_input)

    def _setup_watchtower_input(self, layout: QVBoxLayout) -> None:
        btn_row = QHBoxLayout()
        btn_row.setSpacing(2)
        self._watchtower_btns: list[QPushButton] = []
        for v in [1, 2, 3, 4]:
            btn = QPushButton(str(v))
            btn.setFixedSize(38, 32)
            btn.setCheckable(True)
            btn.setCursor(Qt.CursorShape.PointingHandCursor)
            btn.setStyleSheet(
                "QPushButton { font-size: 16px; font-weight: bold; border-radius: 4px; "
                "border: 2px solid palette(mid); }"
                "QPushButton:checked { background: #7C3AED; color: white; border-color: #7C3AED; }"
            )
            btn.clicked.connect(lambda checked, val=v: self._on_watchtower_selected(val))
            self._watchtower_btns.append(btn)
            btn_row.addWidget(btn)
        layout.addLayout(btn_row)

    def _on_watchtower_selected(self, value: int) -> None:
        for i, btn in enumerate(self._watchtower_btns):
            btn.setChecked(i + 1 == value)
        self.watchtower_changed.emit(value)

    def _setup_symbol_input(self, layout: QVBoxLayout) -> None:
        self._symbol_input = QLineEdit()
        self._symbol_input.setPlaceholderText("符号 (如 ★)")
        self._symbol_input.setMaxLength(2)
        self._symbol_input.textChanged.connect(self._on_symbol_changed)
        layout.addWidget(self._symbol_input)

        quick_row = QHBoxLayout()
        quick_row.setSpacing(2)
        for sym in ["\u2605", "\u25CF", "\u25C6", "\u25B2", "\u2665", "\u25A0"]:
            btn = QPushButton(sym)
            btn.setFixedSize(30, 24)
            btn.setCursor(Qt.CursorShape.PointingHandCursor)
            btn.setStyleSheet(
                "QPushButton { font-size: 12px; border-radius: 3px; padding: 1px; }"
            )
            btn.clicked.connect(lambda checked, s=sym: self._set_symbol(s))
            quick_row.addWidget(btn)
        layout.addLayout(quick_row)

    def _setup_compass_input(self, layout: QVBoxLayout) -> None:
        gl = QGridLayout()
        gl.setSpacing(2)

        val = QIntValidator(-1, 99)
        self._compass_up = QLineEdit()
        self._compass_up.setPlaceholderText("上")
        self._compass_up.setValidator(val)
        self._compass_up.setMaxLength(3)
        self._compass_up.setFixedWidth(52)
        self._compass_up.setAlignment(Qt.AlignmentFlag.AlignCenter)

        self._compass_down = QLineEdit()
        self._compass_down.setPlaceholderText("下")
        self._compass_down.setValidator(val)
        self._compass_down.setMaxLength(3)
        self._compass_down.setFixedWidth(52)
        self._compass_down.setAlignment(Qt.AlignmentFlag.AlignCenter)

        self._compass_left = QLineEdit()
        self._compass_left.setPlaceholderText("左")
        self._compass_left.setValidator(val)
        self._compass_left.setMaxLength(3)
        self._compass_left.setFixedWidth(52)
        self._compass_left.setAlignment(Qt.AlignmentFlag.AlignCenter)

        self._compass_right = QLineEdit()
        self._compass_right.setPlaceholderText("右")
        self._compass_right.setValidator(val)
        self._compass_right.setMaxLength(3)
        self._compass_right.setFixedWidth(52)
        self._compass_right.setAlignment(Qt.AlignmentFlag.AlignCenter)

        center = QLabel("\u25C9")
        center.setAlignment(Qt.AlignmentFlag.AlignCenter)
        center.setStyleSheet("font-size: 14px;")

        gl.addWidget(QLabel("  "), 0, 0)
        gl.addWidget(self._compass_up, 0, 1)
        gl.addWidget(QLabel("  "), 0, 2)
        gl.addWidget(self._compass_left, 1, 0)
        gl.addWidget(center, 1, 1)
        gl.addWidget(self._compass_right, 1, 2)
        gl.addWidget(QLabel("  "), 2, 0)
        gl.addWidget(self._compass_down, 2, 1)

        apply_btn = QPushButton("应用到选中格")
        apply_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        apply_btn.setStyleSheet("QPushButton { font-size: 11px; padding: 4px; }")
        apply_btn.clicked.connect(self._emit_compass)
        layout.addLayout(gl)
        layout.addWidget(apply_btn)

    def _on_mode_selected(self, mode: str) -> None:
        self._current_mode = mode
        self.mode_changed.emit(mode)

    def _on_number_changed(self, text: str) -> None:
        try:
            self.number_changed.emit(int(text) if text else None)
        except ValueError:
            self.number_changed.emit(None)

    def _on_symbol_changed(self, text: str) -> None:
        self.symbol_changed.emit(text)

    def _set_symbol(self, sym: str) -> None:
        self._symbol_input.setText(sym)
        self.symbol_changed.emit(sym)

    def _emit_compass(self) -> None:
        try:
            u = int(self._compass_up.text()) if self._compass_up.text() else -1
            d = int(self._compass_down.text()) if self._compass_down.text() else -1
            l = int(self._compass_left.text()) if self._compass_left.text() else -1
            r = int(self._compass_right.text()) if self._compass_right.text() else -1
        except ValueError:
            return
        clue = CompassClue(up=u, down=d, left=l, right=r)
        self.compass_applied.emit(clue)

    @property
    def current_mode(self) -> str:
        return self._current_mode
