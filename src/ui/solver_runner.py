from __future__ import annotations

from typing import Optional

from PySide6.QtCore import QThread, Signal

from src.models.puzzle import Puzzle
from src.models.solution import Solution
from src.solver.base import default_router


class SolverThread(QThread):
    finished = Signal(object)
    error = Signal(str)

    def __init__(self, puzzle: Puzzle, timeout: float = 30.0,
                 puzzle_name: str | None = None, parent=None) -> None:
        super().__init__(parent)
        self._puzzle = puzzle
        self._timeout = timeout
        self._puzzle_name = puzzle_name
        self._cancelled = False
        self._result: Solution | None = None

    def cancel(self) -> None:
        self._cancelled = True
        if self.isRunning():
            self.quit()
            self.wait(2000)

    def run(self) -> None:
        try:
            router = default_router()
            self._result = router.route(self._puzzle, timeout=self._timeout,
                                        puzzle_name=self._puzzle_name)
            if not self._cancelled:
                self.finished.emit(self._result)
        except Exception as e:
            if not self._cancelled:
                self.error.emit(str(e))
