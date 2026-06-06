from __future__ import annotations

from dataclasses import dataclass

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


@dataclass
class ColorTheme:
    """Semantic colors for QPainter-based widgets (grid, shape editor, preview)."""

    # Grid
    grid_bg: str
    cell_bg_null: str
    cell_border: str
    cell_blocked_bg: str
    cell_blocked_border: str
    cell_blocked_x: str

    # Boundaries
    boundary_edge: str
    boundary_highlight: str
    grid_line: str

    # Edge constraints
    edge_constr_bg: str
    edge_constr_border: str
    edge_constr_text: str

    # Watchtower vertices
    watchtower_bg: str
    watchtower_border: str
    watchtower_text: str

    # Symbol cells
    symbol_bg: str
    symbol_border: str
    symbol_text: str

    # Number text (plain, no symbol)
    number_text: str

    # Compass clues
    compass_text: str
    compass_line: str

    # Shape pattern label
    shape_label: str

    # Selection / hover
    selection_border: str
    selection_vertex_fill: str
    inline_number: str
    hover_cell: str
    hover_vertex: str

    # Rule overlay (right side of grid)
    overlay_bg: tuple[int, int, int, int]  # RGBA
    overlay_border: str
    overlay_text: str
    overlay_header: str

    # Shape pool mini shapes
    shape_mini_pen: str
    shape_mini_fill: str

    # Shape editor widget
    shape_editor_active_bg: str
    shape_editor_active_border: str
    shape_editor_empty_bg: str
    shape_editor_empty_border: str
    shape_editor_area_text: str

    # Puzzle browser preview
    preview_bg: str
    preview_cell_normal: str
    preview_cell_border: str
    preview_blocked_bg: str
    preview_boundary: str
    preview_summary_text: str


LIGHT_COLORS = ColorTheme(
    # Grid
    grid_bg="#FFFFFF",
    cell_bg_null="#F0F0F0",
    cell_border="#E2E8F0",
    cell_blocked_bg="#2C3E50",
    cell_blocked_border="#1A252F",
    cell_blocked_x="#5D6D7E",
    # Boundaries
    boundary_edge="#B8860B",
    boundary_highlight="#FFD700",
    grid_line="#E2E8F0",
    # Edge constraints
    edge_constr_bg="#FFF8E1",
    edge_constr_border="#F59E0B",
    edge_constr_text="#D97706",
    # Watchtower
    watchtower_bg="#EDE9FE",
    watchtower_border="#7C3AED",
    watchtower_text="#7C3AED",
    # Symbol
    symbol_bg="#FEF2F2",
    symbol_border="#FCA5A5",
    symbol_text="#DC2626",
    # Number
    number_text="#1E293B",
    # Compass
    compass_text="#2563EB",
    compass_line="#BFDBFE",
    # Shape pattern
    shape_label="#7C3AED",
    # Selection / hover
    selection_border="#3B82F6",
    selection_vertex_fill="#DBEAFE",
    inline_number="#3B82F6",
    hover_cell="#60A5FA",
    hover_vertex="#60A5FA",
    # Rule overlay
    overlay_bg=(255, 255, 255, 235),
    overlay_border="#D0D5DD",
    overlay_text="#1E293B",
    overlay_header="#64748B",
    # Shape pool mini
    shape_mini_pen="#3B82F6",
    shape_mini_fill="#DBEAFE",
    # Shape editor
    shape_editor_active_bg="#3B82F6",
    shape_editor_active_border="#2563EB",
    shape_editor_empty_bg="#F8FAFC",
    shape_editor_empty_border="#D0D5DD",
    shape_editor_area_text="#475569",
    # Puzzle browser preview
    preview_bg="#FFFFFF",
    preview_cell_normal="#F0F4F8",
    preview_cell_border="#D0D5DD",
    preview_blocked_bg="#2C3E50",
    preview_boundary="#B8860B",
    preview_summary_text="#64748B",
)

DARK_COLORS = ColorTheme(
    # Grid
    grid_bg="#1E1E2E",
    cell_bg_null="#282840",
    cell_border="#3A3A55",
    cell_blocked_bg="#2C3E50",
    cell_blocked_border="#1A252F",
    cell_blocked_x="#5D6D7E",
    # Boundaries
    boundary_edge="#D4A017",
    boundary_highlight="#FFD700",
    grid_line="#3A3A55",
    # Edge constraints
    edge_constr_bg="#3D3520",
    edge_constr_border="#C6890B",
    edge_constr_text="#E5A010",
    # Watchtower
    watchtower_bg="#2A2240",
    watchtower_border="#8B5CF6",
    watchtower_text="#A78BFA",
    # Symbol
    symbol_bg="#3D1F1F",
    symbol_border="#EF4444",
    symbol_text="#FCA5A5",
    # Number
    number_text="#E2E8F0",
    # Compass
    compass_text="#60A5FA",
    compass_line="#1E3A5F",
    # Shape pattern
    shape_label="#A78BFA",
    # Selection / hover
    selection_border="#60A5FA",
    selection_vertex_fill="#1E3A5F",
    inline_number="#60A5FA",
    hover_cell="#60A5FA",
    hover_vertex="#60A5FA",
    # Rule overlay
    overlay_bg=(30, 30, 46, 240),
    overlay_border="#4A4A6A",
    overlay_text="#E2E8F0",
    overlay_header="#94A3B8",
    # Shape pool mini
    shape_mini_pen="#60A5FA",
    shape_mini_fill="#1E3A5F",
    # Shape editor
    shape_editor_active_bg="#3B82F6",
    shape_editor_active_border="#60A5FA",
    shape_editor_empty_bg="#2A2A40",
    shape_editor_empty_border="#4A4A6A",
    shape_editor_area_text="#94A3B8",
    # Puzzle browser preview
    preview_bg="#1E1E2E",
    preview_cell_normal="#2A2A40",
    preview_cell_border="#3A3A55",
    preview_blocked_bg="#2C3E50",
    preview_boundary="#D4A017",
    preview_summary_text="#94A3B8",
)

# Active theme instance, set by apply_theme()
colors: ColorTheme = LIGHT_COLORS


LIGHT_STYLESHEET = """
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

DARK_STYLESHEET = """
QMainWindow {
    background: #1A1A2E;
}

QMenuBar {
    background: #222240;
    border-bottom: 1px solid #3A3A55;
    padding: 1px 0;
    font-size: 13px;
    color: #E2E8F0;
}

QMenuBar::item {
    padding: 5px 14px;
    background: transparent;
    border-radius: 4px;
    margin: 1px 2px;
}

QMenuBar::item:selected {
    background: #2A2A50;
    color: #60A5FA;
}

QMenu {
    background: #222240;
    border: 1px solid #3A3A55;
    border-radius: 8px;
    padding: 6px;
}

QMenu::item {
    padding: 6px 32px 6px 16px;
    border-radius: 4px;
    color: #E2E8F0;
}

QMenu::item:selected {
    background: #2A2A50;
    color: #60A5FA;
}

QMenu::separator {
    height: 1px;
    background: #3A3A55;
    margin: 4px 8px;
}

QStatusBar {
    background: #222240;
    border-top: 1px solid #3A3A55;
    font-size: 12px;
    color: #94A3B8;
    padding: 2px 8px;
}

QSplitter::handle {
    background: #3A3A55;
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
    color: #E2E8F0;
    border: 1px solid #3A3A55;
    border-radius: 6px;
    margin-top: 12px;
    padding: 14px 8px 8px;
    background: #222240;
}

QGroupBox::title {
    subcontrol-origin: margin;
    subcontrol-position: top left;
    padding: 2px 10px;
    background: #222240;
    border: 1px solid #3A3A55;
    border-radius: 4px;
    left: 8px;
    font-size: 12px;
}

QPushButton {
    background: #2A2A40;
    border: 1px solid #4A4A6A;
    border-radius: 6px;
    padding: 6px 14px;
    font-size: 13px;
    color: #E2E8F0;
    min-height: 22px;
}

QPushButton:hover {
    background: #3A3A55;
    border-color: #5B9BD5;
}

QPushButton:pressed {
    background: #2A2A50;
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
    background: #252540;
    color: #5A5A7A;
    border-color: #3A3A55;
}

QSpinBox {
    border: 1px solid #4A4A6A;
    border-radius: 4px;
    padding: 3px 20px 3px 6px;
    background: #2A2A40;
    font-size: 12px;
    color: #E2E8F0;
    min-height: 22px;
    min-width: 80px;
}

QSpinBox:focus {
    border-color: #5B9BD5;
}

QSpinBox:hover {
    border-color: #6A6A8A;
}

QSpinBox::up-button, QSpinBox::down-button {
    width: 18px;
    border-left: 1px solid #4A4A6A;
}

QSpinBox::up-button { subcontrol-position: top right; }
QSpinBox::down-button { subcontrol-position: bottom right; }

QLineEdit {
    border: 1px solid #4A4A6A;
    border-radius: 4px;
    padding: 4px 8px;
    background: #2A2A40;
    font-size: 12px;
    color: #E2E8F0;
    min-height: 20px;
}

QLineEdit:focus {
    border-color: #5B9BD5;
}

QLineEdit:hover {
    border-color: #6A6A8A;
}

QCheckBox {
    font-size: 13px;
    color: #E2E8F0;
    spacing: 8px;
}

QCheckBox::indicator {
    width: 18px;
    height: 18px;
    border: 2px solid #4A4A6A;
    border-radius: 4px;
    background: #2A2A40;
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
    background: #1A1A2E;
    width: 8px;
    border: none;
    margin: 0;
}

QScrollBar::handle:vertical {
    background: #4A4A6A;
    border-radius: 4px;
    min-height: 30px;
}

QScrollBar::handle:vertical:hover {
    background: #5A5A7A;
}

QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {
    height: 0;
}

QScrollBar:horizontal {
    background: #1A1A2E;
    height: 8px;
    border: none;
}

QScrollBar::handle:horizontal {
    background: #4A4A6A;
    border-radius: 4px;
    min-width: 30px;
}

QScrollBar::handle:horizontal:hover {
    background: #5A5A7A;
}

QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {
    width: 0;
}

QLabel {
    color: #E2E8F0;
    font-size: 12px;
}

QListWidget {
    border: 1px solid #4A4A6A;
    border-radius: 6px;
    background: #222240;
    outline: none;
    font-size: 12px;
    color: #E2E8F0;
}

QListWidget::item {
    padding: 6px 10px;
    border-radius: 4px;
}

QListWidget::item:selected {
    background: #2A2A50;
    color: #E2E8F0;
}

QListWidget::item:hover {
    background: #2A2A40;
}

QDialog {
    background: #1A1A2E;
}

QProgressBar {
    background: #3A3A55;
    border: none;
    border-radius: 3px;
    text-align: center;
    color: #E2E8F0;
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
    background: #1A1A2E;
    padding: 3px 3px 0 3px;
}

QTabBar::tab {
    background: #2A2A40;
    border: none;
    border-top-left-radius: 6px;
    border-top-right-radius: 6px;
    padding: 5px 16px;
    margin-right: 2px;
    font-size: 12px;
    font-weight: bold;
    color: #94A3B8;
}

QTabBar::tab:selected {
    background: #222240;
    color: #E2E8F0;
}

QTabBar::tab:hover:!selected {
    background: #3A3A55;
}

QComboBox {
    border: 1px solid #4A4A6A;
    border-radius: 4px;
    padding: 4px 8px;
    background: #2A2A40;
    font-size: 12px;
    color: #E2E8F0;
    min-height: 20px;
}

QComboBox:hover {
    border-color: #6A6A8A;
}

QComboBox QAbstractItemView {
    background: #222240;
    border: 1px solid #4A4A6A;
    selection-background-color: #2A2A50;
    color: #E2E8F0;
}
"""

# Backward-compatible alias
STYLESHEET = LIGHT_STYLESHEET


def apply_theme(app, dark: bool) -> None:
    """Apply light or dark stylesheet and color theme."""
    global colors
    if dark:
        app.setStyleSheet(DARK_STYLESHEET)
        colors = DARK_COLORS
    else:
        app.setStyleSheet(LIGHT_STYLESHEET)
        colors = LIGHT_COLORS
