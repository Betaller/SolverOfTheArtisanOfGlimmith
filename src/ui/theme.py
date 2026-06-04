from __future__ import annotations

REGION_COLORS = [
    "#4E79A7", "#F28E2B", "#59A14F", "#76B7B2", "#499894",
    "#E15759", "#B07AA1", "#FF9DA7", "#9C755F", "#BAB0AC",
    "#86BCB6", "#D37295", "#8CD17D", "#74C476", "#AF7AA1",
    "#79706E", "#D4A6C8", "#A0CBE8", "#FFBE7D", "#CAB2D6",
]

MODE_COLORS = {
    "select": "#5B9BD5",
    "boundary": "#E74C3C",
    "block": "#7F8C8D",
    "number": "#27AE60",
    "symbol": "#8E44AD",
    "compass": "#2980B9",
    "watchtower": "#D35400",
}

RULE_CATEGORIES: list[tuple[str, list[str]]] = [
    ("形状类", ["shape_pool", "same", "different", "mixed", "block", "non_block"]),
    ("面积类", ["precise", "range", "area", "differentiation"]),
    ("约束边", ["heterogeneous", "homogeneous", "inequality", "difference"]),
    ("标记类", ["puzzle_piece", "fence", "solitary", "watchtower", "compass", "rose_window"]),
    ("结构类", ["brick", "ring"]),
]

STYLESHEET = """
QMainWindow {
    background: #F0F2F5;
}

QMenuBar {
    background: #FFFFFF;
    border-bottom: 1px solid #E0E3E8;
    padding: 1px 0;
    font-size: 13px;
    color: #2C3E50;
}

QMenuBar::item {
    padding: 5px 14px;
    background: transparent;
    border-radius: 4px;
    margin: 1px 2px;
}

QMenuBar::item:selected {
    background: #EBF5FB;
    color: #2980B9;
}

QMenu {
    background: #FFFFFF;
    border: 1px solid #E0E3E8;
    border-radius: 8px;
    padding: 6px;
}

QMenu::item {
    padding: 6px 32px 6px 16px;
    border-radius: 4px;
}

QMenu::item:selected {
    background: #EBF5FB;
    color: #2980B9;
}

QMenu::separator {
    height: 1px;
    background: #E0E3E8;
    margin: 4px 8px;
}

QStatusBar {
    background: #FFFFFF;
    border-top: 1px solid #E0E3E8;
    font-size: 12px;
    color: #7F8C8D;
    padding: 2px 8px;
}

QSplitter::handle {
    background: #E0E3E8;
}

QSplitter::handle:horizontal {
    width: 1px;
}

QSplitter::handle:vertical {
    height: 1px;
}

QGroupBox {
    font-size: 12px;
    font-weight: bold;
    color: #2C3E50;
    border: 1px solid #E0E3E8;
    border-radius: 6px;
    margin-top: 12px;
    padding: 14px 8px 8px;
    background: #FFFFFF;
}

QGroupBox::title {
    subcontrol-origin: margin;
    subcontrol-position: top left;
    padding: 2px 10px;
    background: #FFFFFF;
    border: 1px solid #E0E3E8;
    border-radius: 4px;
    left: 8px;
    font-size: 12px;
}

QPushButton {
    background: #FFFFFF;
    border: 1px solid #D0D5DD;
    border-radius: 6px;
    padding: 6px 14px;
    font-size: 13px;
    color: #2C3E50;
    min-height: 22px;
}

QPushButton:hover {
    background: #F5F8FF;
    border-color: #5B9BD5;
}

QPushButton:pressed {
    background: #EBF5FB;
}

QPushButton:checked {
    background: #5B9BD5;
    color: #FFFFFF;
    border-color: #4A8BC7;
}

QPushButton:checked:hover {
    background: #4A8BC7;
}

QPushButton:disabled {
    background: #F5F6FA;
    color: #BDC3C7;
    border-color: #E0E3E8;
}

QSpinBox {
    border: 1px solid #D0D5DD;
    border-radius: 4px;
    padding: 3px 20px 3px 6px;
    background: #FFFFFF;
    font-size: 12px;
    color: #2C3E50;
    min-height: 22px;
    min-width: 80px;
}

QSpinBox:focus {
    border-color: #5B9BD5;
}

QSpinBox:hover {
    border-color: #A0AAB8;
}

QSpinBox::up-button, QSpinBox::down-button {
    width: 18px;
    border-left: 1px solid #D0D5DD;
}

QSpinBox::up-button { subcontrol-position: top right; }
QSpinBox::down-button { subcontrol-position: bottom right; }

QLineEdit {
    border: 1px solid #D0D5DD;
    border-radius: 4px;
    padding: 4px 8px;
    background: #FFFFFF;
    font-size: 12px;
    color: #2C3E50;
    min-height: 20px;
}

QLineEdit:focus {
    border-color: #5B9BD5;
}

QLineEdit:hover {
    border-color: #A0AAB8;
}

QCheckBox {
    font-size: 13px;
    color: #2C3E50;
    spacing: 8px;
}

QCheckBox::indicator {
    width: 18px;
    height: 18px;
    border: 2px solid #D0D5DD;
    border-radius: 4px;
    background: #FFFFFF;
}

QCheckBox::indicator:hover {
    border-color: #5B9BD5;
}

QCheckBox::indicator:checked {
    background: #5B9BD5;
    border-color: #5B9BD5;
}

QScrollArea {
    border: none;
    background: transparent;
}

QScrollBar:vertical {
    background: #F0F2F5;
    width: 8px;
    border: none;
    margin: 0;
}

QScrollBar::handle:vertical {
    background: #D0D5DD;
    border-radius: 4px;
    min-height: 30px;
}

QScrollBar::handle:vertical:hover {
    background: #A0AAB8;
}

QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {
    height: 0;
}

QScrollBar:horizontal {
    background: #F0F2F5;
    height: 8px;
    border: none;
}

QScrollBar::handle:horizontal {
    background: #D0D5DD;
    border-radius: 4px;
    min-width: 30px;
}

QScrollBar::handle:horizontal:hover {
    background: #A0AAB8;
}

QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {
    width: 0;
}

QLabel {
    color: #2C3E50;
    font-size: 12px;
}

QListWidget {
    border: 1px solid #D0D5DD;
    border-radius: 6px;
    background: #FFFFFF;
    outline: none;
    font-size: 12px;
}

QListWidget::item {
    padding: 6px 10px;
    border-radius: 4px;
}

QListWidget::item:selected {
    background: #EBF5FB;
    color: #2C3E50;
}

QListWidget::item:hover {
    background: #F5F8FF;
}

QDialog {
    background: #F0F2F5;
}

QProgressBar {
    background: #E0E3E8;
    border: none;
    border-radius: 3px;
    text-align: center;
}

QProgressBar::chunk {
    background: #3B82F6;
    border-radius: 3px;
}

QTabWidget::pane {
    border: none;
    background: transparent;
}

QTabBar {
    background: #F0F2F5;
    padding: 3px 3px 0 3px;
}

QTabBar::tab {
    background: #E2E5EA;
    border: none;
    border-top-left-radius: 6px;
    border-top-right-radius: 6px;
    padding: 5px 16px;
    margin-right: 2px;
    font-size: 12px;
    font-weight: bold;
    color: #64748B;
}

QTabBar::tab:selected {
    background: #FFFFFF;
    color: #1E293B;
}

QTabBar::tab:hover:!selected {
    background: #D0D5DD;
}
"""
