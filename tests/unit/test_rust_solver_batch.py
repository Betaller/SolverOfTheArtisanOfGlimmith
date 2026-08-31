"""Tests for RustSolver.solve_batch, focused on the stderr-deadlock bug (L6)."""
from __future__ import annotations

import json
import subprocess
import sys

import pytest

from src.models.board import Board
from src.models.puzzle import Puzzle
from src.solver.rust_solver import RustSolver


def _make_puzzle() -> Puzzle:
    return Puzzle.from_board(Board(2, 2))


def test_solve_batch_no_deadlock_when_stderr_flooded(monkeypatch) -> None:
    """Bug L6: solve_batch opened stderr=PIPE but never read it, so a subprocess
    that wrote more than the 64KB pipe buffer would block and the whole batch
    looked "timed out".  With stderr=DEVNULL there is nothing to fill, so the
    batch returns promptly even when the child floods stderr.
    """
    # The constructor looks for the real binary; we never actually launch it.
    monkeypatch.setattr(
        "src.solver.rust_solver._find_binary", lambda: "/tmp/dummy_rsolver"
    )

    script = (
        "import sys, json; "
        "sys.stdout.write(json.dumps({'solved': False, 'error_message': 'x'}) + '\\n'); "
        "sys.stdout.flush(); "
        "sys.stderr.write('y' * 200000); "
        "sys.stderr.flush()"
    )

    def fake_popen(args, **kwargs):
        # Ignore the requested binary; run a helper that floods stderr and emits
        # exactly one JSON result line on stdout.
        return subprocess.Popen([sys.executable, "-c", script], **kwargs)

    monkeypatch.setattr("src.solver.rust_solver.subprocess.Popen", fake_popen)

    solver = RustSolver()
    results = solver.solve_batch([_make_puzzle(), _make_puzzle()], timeout=5)

    assert len(results) == 2
    assert all(not r.solved for r in results)
