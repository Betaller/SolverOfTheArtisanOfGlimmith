from __future__ import annotations

import os
import copy
from typing import Optional

from PySide6.QtCore import Qt, QTimer
from PySide6.QtWidgets import (
    QMainWindow, QSplitter, QTabWidget, QWidget, QVBoxLayout,
    QHBoxLayout, QStatusBar, QMenuBar, QMenu, QFileDialog,
    QMessageBox, QLabel, QPushButton, QSpinBox, QProgressBar,
    QDialog, QFormLayout, QDialogButtonBox, QFrame,
)
from PySide6.QtGui import QAction, QKeyEvent

from src.models.board import Board, CompassClue
from src.models.puzzle import Puzzle
from src.models.solution import Solution
from src.services.puzzle_service import PuzzleService
from src.services.solver_service import SolverService
from src.io.puzzle_codec import puzzle_to_dict, dict_to_puzzle
from src.ui.grid_widget import GridWidget
from src.ui.constraint_panel import ConstraintPanel
from src.ui.tool_palette import ToolPalette
from src.ui.property_panel import PropertyPanel
from src.ui.puzzle_browser import PuzzleBrowser
from src.ui.solver_runner import SolverThread
from src.ui.theme import MODE_COLORS


class NewPuzzleDialog(QDialog):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setWindowTitle("新建谜题")
        self.setFixedSize(280, 150)
        layout = QFormLayout(self)
        layout.setContentsMargins(20, 20, 20, 20)
        layout.setSpacing(12)

        self._height_spin = QSpinBox()
        self._height_spin.setRange(2, 16)
        self._height_spin.setValue(6)
        self._height_spin.setFixedWidth(80)
        layout.addRow("高度:", self._height_spin)

        self._width_spin = QSpinBox()
        self._width_spin.setRange(2, 16)
        self._width_spin.setValue(6)
        self._width_spin.setFixedWidth(80)
        layout.addRow("宽度:", self._width_spin)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addRow(buttons)

    @property
    def height(self) -> int:
        return self._height_spin.value()

    @property
    def width(self) -> int:
        return self._width_spin.value()


SOLVE_BTN_STYLE = """
QPushButton {
    font-size: 15px;
    font-weight: bold;
    padding: 12px;
    background: #10B981;
    color: white;
    border: none;
    border-radius: 8px;
}
QPushButton:hover {
    background: #059669;
}
QPushButton:pressed {
    background: #047857;
}
"""


class MainWindow(QMainWindow):
    def __init__(self) -> None:
        super().__init__()
        self._puzzle_service = PuzzleService()
        self._solver_service = SolverService()
        self._puzzle: Puzzle | None = None
        self._current_file: str | None = None
        self._initial_puzzle_data: dict | None = None
        self._undo_stack: list[dict] = []
        self._redo_stack: list[dict] = []
        self._undo_depth = 100

        self.setWindowTitle("格里米斯的工匠 - 求解器")
        self.resize(1320, 840)

        self._setup_status_bar()
        self._setup_ui()
        self._setup_menu()

        self._create_new_puzzle(6, 6)

    def _setup_ui(self) -> None:
        central = QWidget()
        self.setCentralWidget(central)
        main_layout = QVBoxLayout(central)
        main_layout.setContentsMargins(0, 0, 0, 0)
        main_layout.setSpacing(0)

        content = QSplitter(Qt.Orientation.Horizontal)

        left_panel = QTabWidget()
        left_panel.setTabPosition(QTabWidget.TabPosition.North)
        left_panel.setMinimumWidth(200)
        left_panel.setMaximumWidth(280)
        self._tool_palette = ToolPalette()
        self._constraint_panel = ConstraintPanel()
        self._puzzle_browser = PuzzleBrowser()
        left_panel.addTab(self._tool_palette, "工具")
        left_panel.addTab(self._constraint_panel, "规则配置")
        left_panel.addTab(self._puzzle_browser, "谜题列表")

        self._grid_widget = GridWidget()
        self._grid_widget.setMinimumWidth(400)

        right_panel = QSplitter(Qt.Orientation.Vertical)
        self._property_panel = PropertyPanel()
        self._property_panel.setMinimumWidth(240)
        self._property_panel.setMaximumWidth(320)

        control_widget = QWidget()
        control_layout = QVBoxLayout(control_widget)
        control_layout.setContentsMargins(8, 8, 8, 8)
        control_layout.setSpacing(8)

        self._solve_btn = QPushButton("求解")
        self._solve_btn.setStyleSheet(SOLVE_BTN_STYLE)
        self._solve_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._solve_btn.clicked.connect(self._on_solve)
        control_layout.addWidget(self._solve_btn)

        self._progress_bar = QProgressBar()
        self._progress_bar.setRange(0, 0)
        self._progress_bar.setFixedHeight(6)
        self._progress_bar.setTextVisible(False)
        self._progress_bar.setVisible(False)
        self._progress_bar.setStyleSheet(
            "QProgressBar { border: none; border-radius: 3px; }"
            "QProgressBar::chunk { background: #3B82F6; border-radius: 3px; }"
        )
        control_layout.addWidget(self._progress_bar)

        reset_btn = QPushButton("重置")
        reset_btn.setStyleSheet(
            "QPushButton { font-size: 13px; padding: 8px; border-radius: 6px; }"
        )
        reset_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        reset_btn.clicked.connect(self._on_reset)
        control_layout.addWidget(reset_btn)

        self._result_label = QLabel("就绪")
        self._result_label.setWordWrap(True)
        self._result_label.setStyleSheet(
            "font-size: 12px; padding: 8px; "
            "border: 1px solid palette(mid); border-radius: 6px;"
        )
        self._result_label.setAlignment(Qt.AlignmentFlag.AlignTop)
        control_layout.addWidget(self._result_label)
        control_layout.addStretch()

        right_panel.addWidget(self._property_panel)
        right_panel.addWidget(control_widget)
        right_panel.setStretchFactor(0, 2)
        right_panel.setStretchFactor(1, 1)

        content.addWidget(left_panel)
        content.addWidget(self._grid_widget)
        content.addWidget(right_panel)
        content.setStretchFactor(0, 0)
        content.setStretchFactor(1, 1)
        content.setStretchFactor(2, 0)
        content.setHandleWidth(1)

        main_layout.addWidget(content)

        self._tool_palette.mode_changed.connect(self._grid_widget.set_mode)
        self._tool_palette.number_changed.connect(self._grid_widget.set_number)
        self._tool_palette.symbol_changed.connect(self._grid_widget.set_symbol)
        self._tool_palette.compass_applied.connect(self._on_compass_applied)
        self._tool_palette.watchtower_changed.connect(self._on_watchtower_changed)
        self._tool_palette.grid_size_changed.connect(self._on_grid_size_changed)
        self._grid_widget.cell_clicked.connect(self._on_cell_clicked)
        self._grid_widget.edge_clicked.connect(self._on_edge_clicked)
        self._grid_widget.vertex_clicked.connect(self._on_vertex_clicked)
        self._grid_widget.mode_changed.connect(self._on_grid_mode_changed)
        self._grid_widget.status_message.connect(self._status_label.setText)
        self._grid_widget.board_modified.connect(self._on_board_modified)
        self._property_panel.board_modified.connect(self._grid_widget.update)
        self._property_panel.board_modified.connect(self._on_board_modified)
        self._constraint_panel.rules_changed.connect(self._on_rules_changed)
        self._puzzle_browser.puzzle_selected.connect(self._on_puzzle_browser_selected)

    def _on_compass_applied(self, clue: CompassClue) -> None:
        self._grid_widget.set_compass(clue)

    def _on_watchtower_changed(self, value: int) -> None:
        self._grid_widget.set_number(value)

    def _on_grid_size_changed(self, w: int, h: int) -> None:
        self._create_new_puzzle(h, w)

    def _on_grid_mode_changed(self, mode: str) -> None:
        names = {
            "select": "选择",
            "boundary": "边框绘制",
            "block": "障碍格",
            "number": "数字标注",
            "symbol": "符号标注",
            "compass": "罗盘标注",
            "watchtower": "望塔标注",
        }
        name = names.get(mode, mode)
        self._status_label.setText(f"当前模式: {name}")

    def _setup_menu(self) -> None:
        menubar = self.menuBar()

        file_menu = menubar.addMenu("文件")

        new_action = QAction("新建", self)
        new_action.setShortcut("Ctrl+N")
        new_action.triggered.connect(self._on_new)
        file_menu.addAction(new_action)

        open_action = QAction("打开", self)
        open_action.setShortcut("Ctrl+O")
        open_action.triggered.connect(self._on_open)
        file_menu.addAction(open_action)

        save_action = QAction("保存", self)
        save_action.setShortcut("Ctrl+S")
        save_action.triggered.connect(self._on_save)
        file_menu.addAction(save_action)

        save_as_action = QAction("另存为", self)
        save_as_action.setShortcut("Ctrl+Shift+S")
        save_as_action.triggered.connect(self._on_save_as)
        file_menu.addAction(save_as_action)

        file_menu.addSeparator()
        undo_action = QAction("撤销", self)
        undo_action.setShortcut("Ctrl+Z")
        undo_action.triggered.connect(self._on_undo)
        file_menu.addAction(undo_action)

        redo_action = QAction("重做", self)
        redo_action.setShortcut("Ctrl+Shift+Z")
        redo_action.triggered.connect(self._on_redo)
        file_menu.addAction(redo_action)

        file_menu.addSeparator()
        reset_action = QAction("重置", self)
        reset_action.setShortcut("Ctrl+R")
        reset_action.triggered.connect(self._on_reset)
        file_menu.addAction(reset_action)

        file_menu.addSeparator()
        exit_action = QAction("退出", self)
        exit_action.triggered.connect(self.close)
        file_menu.addAction(exit_action)

        solve_menu = menubar.addMenu("求解")
        solve_action = QAction("开始求解", self)
        solve_action.setShortcut("F5")
        solve_action.triggered.connect(self._on_solve)
        solve_menu.addAction(solve_action)

        help_menu = menubar.addMenu("帮助")
        about_action = QAction("关于", self)
        about_action.triggered.connect(self._show_about)
        help_menu.addAction(about_action)

    def _setup_status_bar(self) -> None:
        self._status_label = QLabel("就绪")
        self.statusBar().addWidget(self._status_label)

    def _save_undo_snapshot(self) -> None:
        """Save current puzzle state for undo before a modification."""
        self._sync_puzzle_from_ui()
        if self._puzzle is None:
            return
        snap = puzzle_to_dict(self._puzzle)
        if self._undo_stack and snap == self._undo_stack[-1]:
            return
        self._undo_stack.append(snap)
        if len(self._undo_stack) > self._undo_depth:
            self._undo_stack.pop(0)
        self._redo_stack.clear()

    def _on_undo(self) -> None:
        if not self._undo_stack:
            return
        self._sync_puzzle_from_ui()
        cur = puzzle_to_dict(self._puzzle) if self._puzzle else None
        prev = self._undo_stack.pop()
        if cur is not None:
            self._redo_stack.append(cur)
        self._apply_snapshot(prev)
        self._status_label.setText("已撤销")

    def _on_redo(self) -> None:
        if not self._redo_stack:
            return
        self._sync_puzzle_from_ui()
        cur = puzzle_to_dict(self._puzzle) if self._puzzle else None
        nxt = self._redo_stack.pop()
        if cur is not None:
            self._undo_stack.append(cur)
        self._apply_snapshot(nxt)
        self._status_label.setText("已重做")

    def _on_board_modified(self) -> None:
        """Debounced undo snapshot save after user modifications."""
        if hasattr(self, '_undo_timer'):
            self._undo_timer.stop()
        else:
            self._undo_timer = QTimer()
            self._undo_timer.setSingleShot(True)
            self._undo_timer.timeout.connect(self._save_undo_snapshot)
        self._undo_timer.start(300)

    def _apply_snapshot(self, data: dict) -> None:
        self._puzzle = dict_to_puzzle(data)
        board = Board(self._puzzle.height, self._puzzle.width)
        self._copy_puzzle_to_board(board)
        self._grid_widget.set_board(board)
        self._property_panel.set_board(board)
        self._constraint_panel.set_puzzle(self._puzzle)
        self._update_title()
        self._update_overlay()

    def _show_about(self) -> None:
        QMessageBox.about(self, "关于",
            "格里米斯的工匠 - 求解器 v0.1.0\n\n"
            "基于回溯搜索 + 约束传播的自动求解工具。\n"
            "支持全部 22 条规则。\n\n"
            "Powered by PySide6"
        )

    def _update_overlay(self) -> None:
        if self._puzzle is None:
            self._grid_widget.set_overlay_info([], [])
            return
        from src.models.puzzle import RULE_NAMES
        rule_names = list(dict.fromkeys(RULE_NAMES.get(r.type, r.type) for r in self._puzzle.rules))
        shapes = []
        pool_rule = self._puzzle.get_rule("shape_pool")
        if pool_rule is not None:
            shapes = pool_rule.params.get("shapes", [])
        self._grid_widget.set_overlay_info(rule_names, shapes)

    def _save_initial_state(self) -> None:
        if self._puzzle is not None:
            self._initial_puzzle_data = puzzle_to_dict(self._puzzle)

    def _load_from_initial(self) -> None:
        if self._initial_puzzle_data is None:
            return
        self._puzzle = dict_to_puzzle(self._initial_puzzle_data)
        board = Board(self._puzzle.height, self._puzzle.width)
        self._copy_puzzle_to_board(board)
        self._grid_widget.set_board(board)
        self._property_panel.set_board(board)
        self._constraint_panel.set_puzzle(self._puzzle)
        self._result_label.setText("已重置")
        self._status_label.setText("已重置为初始状态")
        self._update_title()
        self._update_overlay()

    def _on_reset(self) -> None:
        if self._initial_puzzle_data is None:
            return
        if hasattr(self, '_solver_thread') and self._solver_thread is not None and self._solver_thread.isRunning():
            self._solver_thread.cancel()
            self._solve_btn.setEnabled(True)
            self._solve_btn.setText("求解")
        self._load_from_initial()

    def _create_new_puzzle(self, height: int, width: int) -> None:
        self._puzzle = self._puzzle_service.create_puzzle(height, width)
        self._current_file = None
        self._save_initial_state()

        board = Board(height, width)
        self._grid_widget.set_board(board)
        self._property_panel.set_board(board)
        self._constraint_panel.set_puzzle(self._puzzle)
        self._result_label.setText("就绪")
        self._status_label.setText(f"新建谜题: {height}x{width}")
        self._update_title()
        self._update_overlay()

    def _update_title(self) -> None:
        name = os.path.basename(self._current_file) if self._current_file else "未命名"
        self.setWindowTitle(f"格里米斯的工匠 - 求解器 ({name})")

    def _on_new(self) -> None:
        dialog = NewPuzzleDialog(self)
        if dialog.exec():
            self._create_new_puzzle(dialog.height, dialog.width)

    def _on_open(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, "打开谜题", "", "谜题文件 (*.json);;所有文件 (*)",
        )
        if not path:
            return
        self._load_puzzle_file(path)

    def _on_puzzle_browser_selected(self, path: str) -> None:
        self._load_puzzle_file(path)

    def _load_puzzle_file(self, path: str) -> None:
        try:
            self._puzzle = self._puzzle_service.load_puzzle(path)
            self._current_file = path

            board = Board(self._puzzle.height, self._puzzle.width)
            self._copy_puzzle_to_board(board)

            self._grid_widget.set_board(board)
            self._property_panel.set_board(board)
            self._constraint_panel.set_puzzle(self._puzzle)
            self._save_initial_state()
            self._result_label.setText("已加载")
            self._status_label.setText(f"已加载: {os.path.basename(path)}")
            self._update_title()
            self._update_overlay()
        except Exception as e:
            QMessageBox.critical(self, "加载失败", str(e))

    def _on_save(self) -> None:
        if self._puzzle is None:
            return
        self._sync_puzzle_from_ui()
        if self._current_file:
            self._puzzle_service.save_puzzle(self._puzzle, self._current_file)
            self._status_label.setText(f"已保存: {os.path.basename(self._current_file)}")
        else:
            self._on_save_as()

    def _on_save_as(self) -> None:
        if self._puzzle is None:
            return
        self._sync_puzzle_from_ui()
        path, _ = QFileDialog.getSaveFileName(
            self, "保存谜题", "puzzles/user/", "谜题文件 (*.json);;所有文件 (*)",
        )
        if not path:
            return
        self._current_file = path
        self._on_save()
        self._update_title()

    def _copy_puzzle_to_board(self, board: Board) -> None:
        for c in self._puzzle.cells:
            dst = board.cell(c.row, c.col)
            dst.number = c.number
            dst.symbol = c.symbol
            dst.shape_pattern = c.shape_pattern
            dst.compass = c.compass
            dst.fence_pattern = c.fence_pattern
            dst.blocked = c.blocked
        for e in self._puzzle.edges:
            edge = board.edge_between(e.r1, e.c1, e.r2, e.c2)
            if edge is not None:
                edge.is_boundary = e.is_boundary
                edge.constraint = e.constraint
        for v in self._puzzle.vertices:
            vert = board.vertex_at(v.row, v.col)
            if vert is not None:
                vert.watchtower = v.watchtower
        board.outer_boundaries = list(self._puzzle.outer_boundaries)

    def _sync_puzzle_from_ui(self) -> None:
        if self._puzzle is None or self._grid_widget.board is None:
            return
        src = self._grid_widget.board
        for c in self._puzzle.cells:
            dst = src.cell(c.row, c.col)
            c.number = dst.number
            c.symbol = dst.symbol
            c.shape_pattern = dst.shape_pattern
            c.compass = dst.compass
            c.fence_pattern = dst.fence_pattern
            c.blocked = dst.blocked
        for e in self._puzzle.edges:
            edge = src.edge_between(e.r1, e.c1, e.r2, e.c2)
            if edge is not None:
                e.is_boundary = edge.is_boundary
                e.constraint = edge.constraint
        for v in self._puzzle.vertices:
            vert = src.vertex_at(v.row, v.col)
            if vert is not None:
                v.watchtower = vert.watchtower
        self._puzzle.outer_boundaries = list(src.outer_boundaries)

    def _on_cell_clicked(self, r: int, c: int) -> None:
        self._property_panel.select_cell(r, c)

    def _on_edge_clicked(self, r1: int, c1: int, r2: int, c2: int) -> None:
        self._property_panel.select_edge(r1, c1, r2, c2)

    def _on_vertex_clicked(self, r: int, c: int) -> None:
        self._property_panel.select_vertex(r, c)

    def _on_rules_changed(self) -> None:
        self._update_overlay()
        if self._puzzle:
            warnings = self._puzzle_service.validate_rules(self._puzzle)
            if warnings:
                self._status_label.setText("; ".join(warnings))
            else:
                self._status_label.setText("规则配置无误")

    def _on_solve(self) -> None:
        if self._puzzle is None:
            QMessageBox.warning(self, "提示", "请先创建或加载谜题")
            return

        if not self._puzzle.rules:
            QMessageBox.warning(self, "提示", "请至少启用一条规则")
            return

        if hasattr(self, '_solver_thread') and self._solver_thread is not None and self._solver_thread.isRunning():
            self._solver_thread.cancel()
            self._solve_btn.setEnabled(True)
            self._solve_btn.setText("求解")
            self._progress_bar.setVisible(False)
            self._status_label.setText("求解已取消")
            self._result_label.setText("求解已取消")
            return

        self._sync_puzzle_from_ui()

        self._solve_btn.setText("取消求解")
        self._progress_bar.setVisible(True)
        self._result_label.setText('<span style="color: #3B82F6;">求解中...</span>')
        self._result_label.setTextFormat(Qt.TextFormat.RichText)
        self._status_label.setText("求解中...")

        self._solver_thread = SolverThread(self._puzzle, timeout=30)
        self._solver_thread.finished.connect(self._on_solution_ready)
        self._solver_thread.error.connect(self._on_solver_error)
        self._solver_thread.start()

    def _on_solution_ready(self, solution: Solution) -> None:
        if self._puzzle is None:
            return

        board = Board(self._puzzle.height, self._puzzle.width)
        self._copy_puzzle_to_board(board)

        if solution.solved:
            for region in solution.regions:
                for r, c in region.cells:
                    board.cell(r, c).region_id = region.region_id

            for e in board.edges():
                c1 = board.cell(e.r1, e.c1)
                c2 = board.cell(e.r2, e.c2)
                if c1.assigned and c2.assigned:
                    e.is_boundary = c1.region_id != c2.region_id

            self._status_label.setText(
                f"求解成功! {solution.elapsed_ms}ms, {solution.steps_taken}步, {len(solution.regions)}个区域"
            )
            self._result_label.setText(
                f'<b style="color: #059669; font-size: 14px;">求解成功!</b><br>'
                f'<span style="color: #64748B;">耗时:</span> {solution.elapsed_ms}ms<br>'
                f'<span style="color: #64748B;">搜索步数:</span> {solution.steps_taken}<br>'
                f'<span style="color: #64748B;">区域数:</span> {len(solution.regions)}'
            )
            self._result_label.setTextFormat(Qt.TextFormat.RichText)
        else:
            self._status_label.setText(
                f"求解失败: {solution.error_message or '无解'}"
            )
            self._result_label.setText(
                f'<b style="color: #DC2626; font-size: 14px;">求解失败</b><br>'
                f'<span style="color: #64748B;">原因:</span> {solution.error_message or "无解"}<br>'
                f'<span style="color: #64748B;">耗时:</span> {solution.elapsed_ms}ms'
            )
            self._result_label.setTextFormat(Qt.TextFormat.RichText)

        self._grid_widget.set_board(board)
        self._property_panel.set_board(board)
        self._solve_btn.setEnabled(True)
        self._solve_btn.setText("求解")
        self._progress_bar.setVisible(False)

    def _on_solver_error(self, err_msg: str) -> None:
        QMessageBox.critical(self, "求解错误", err_msg)
        self._status_label.setText("求解出错")
        self._result_label.setText(f"出错: {err_msg}")
        self._solve_btn.setEnabled(True)
        self._solve_btn.setText("求解")
        self._progress_bar.setVisible(False)

    def keyPressEvent(self, event: QKeyEvent) -> None:
        key_map = {
            Qt.Key.Key_V: "select",
            Qt.Key.Key_B: "boundary",
            Qt.Key.Key_X: "block",
            Qt.Key.Key_N: "number",
            Qt.Key.Key_S: "symbol",
            Qt.Key.Key_C: "compass",
            Qt.Key.Key_W: "watchtower",
        }
        if event.key() in key_map:
            self._tool_palette._on_mode_selected(key_map[event.key()])
        elif event.key() == Qt.Key.Key_F5:
            self._on_solve()
        super().keyPressEvent(event)
