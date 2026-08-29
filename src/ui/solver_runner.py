from __future__ import annotations

from PySide6.QtCore import QThread, Signal

from src.models.puzzle import Puzzle
from src.models.solution import Solution
from src.solver.base import SolverRouter, default_router


class SolverThread(QThread):
    finished = Signal(object)
    error = Signal(str)

    def __init__(
        self, puzzle: Puzzle, timeout: float = 30.0, puzzle_name: str | None = None, parent=None
    ) -> None:
        super().__init__(parent)
        self._puzzle = puzzle
        self._timeout = timeout
        self._puzzle_name = puzzle_name
        self._cancelled = False
        self._router: SolverRouter | None = None
        self._result: Solution | None = None

    def cancel(self) -> None:
        """Stop the solve for real: kill the subprocess, then let the thread end.

        `quit()` alone never did anything — `run` has no event loop — so
        cancelling used to block the GUI for the full `wait()` and then leave
        the rsolver subprocess burning CPU until it hit its own 144s deadline.
        The process is now killed up front, so `wait` returns as soon as the
        pump thread notices the pipes closed (one poll interval, ~50ms).
        """
        self._cancelled = True
        if self._router is not None:
            self._router.cancel()
        if self.isRunning():
            self.wait(2000)

    def run(self) -> None:
        try:
            router = default_router()
            self._router = router
            # `cancel` may have landed before the router existed, in which case
            # it could only set the flag and never reached a subprocess.
            if self._cancelled:
                router.cancel()
                return
            self._result = router.route(
                self._puzzle, timeout=self._timeout, puzzle_name=self._puzzle_name
            )
            if not self._cancelled:
                self.finished.emit(self._result)
        except Exception as e:
            if not self._cancelled:
                self.error.emit(str(e))
