"""Headless (offscreen) tests for the UI bug fixes C1, C3, L7.

These exercise the Qt widgets directly.  They require PySide6; run with the
offscreen platform plugin so no display is needed.
"""
from __future__ import annotations

import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import json
import logging
from pathlib import Path
from unittest.mock import patch

import pytest
from PySide6.QtWidgets import QApplication

from src.models.board import Board
from src.models.puzzle import Puzzle, Rule


@pytest.fixture(scope="session")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    yield app


def test_set_puzzle_does_not_duplicate_rules(qapp) -> None:
    """Bug C1: set_puzzle must not re-append rules already present in the loaded
    puzzle (the checkbox `setChecked` used to fire `toggled` → `_on_rule_toggled`
    which appended the rule again, producing [area,block,area,block]).
    """
    from src.ui.constraint_panel import ConstraintPanel

    panel = ConstraintPanel()
    puzzle = Puzzle.from_board(
        Board(4, 4), rules=[Rule(type="area"), Rule(type="block")]
    )
    panel.set_puzzle(puzzle)
    assert len(puzzle.rules) == 2
    assert puzzle.rules.count(Rule(type="area")) == 1
    assert puzzle.rules.count(Rule(type="block")) == 1

    # Loading the same puzzle again (reset / undo-redo path) must not duplicate.
    panel.set_puzzle(puzzle)
    assert len(puzzle.rules) == 2


def test_toggling_rule_on_populates_default_params(qapp) -> None:
    """Bug C3: toggling a parameterized rule ON must seed its params from the
    spin widgets, otherwise an empty params dict resolves `area`/`min` to 0 and
    makes the puzzle silently unsolvable.
    """
    from src.ui.constraint_panel import ConstraintPanel

    panel = ConstraintPanel()
    puzzle = Puzzle.from_board(Board(4, 4))
    panel.set_puzzle(puzzle)

    panel._on_rule_toggled("precise", True)
    rule = puzzle.get_rule("precise")
    assert rule is not None
    assert rule.params.get("area", 0) != 0
    assert rule.params["area"] == 1

    # Toggling OFF must remove it again.
    panel._on_rule_toggled("precise", False)
    assert puzzle.get_rule("precise") is None


def test_puzzle_browser_keys_by_path_and_surfaces_errors(qapp, tmp_path, caplog) -> None:
    """Bug L7: the cache/lookup must be keyed by full path (same basename in
    different dirs must not collide), and corrupt JSON must be surfaced (logged)
    rather than silently swallowed.
    """
    import src.ui.puzzle_browser as pb

    (tmp_path / "cat1").mkdir()
    (tmp_path / "cat2").mkdir()
    f1 = tmp_path / "cat1" / "p.json"
    f1.write_text(
        json.dumps({"grid": {"height": 2, "width": 2}, "rules": [], "cells": [], "edges": []})
    )
    f2 = tmp_path / "cat2" / "p.json"  # same basename, different directory
    f2.write_text(
        json.dumps({"grid": {"height": 3, "width": 3}, "rules": [], "cells": [], "edges": []})
    )
    bad = tmp_path / "cat1" / "bad.json"
    bad.write_text("{ not valid json")

    with patch.object(pb, "PUZZLE_BASE", str(tmp_path)):
        with caplog.at_level(logging.WARNING, logger=pb.logger.name):
            browser = pb.PuzzleBrowser()

    # Cache keyed by full path: the two same-basename files do not collide.
    assert str(f1) in browser._grid_cache
    assert str(f2) in browser._grid_cache
    assert len(browser._grid_cache) == 2

    # Lookup resolves unambiguously by full path.
    info = browser._find_info(str(f2))
    assert info is not None and info.path == str(f2)

    # The corrupt file is skipped but its failure is surfaced (not swallowed).
    assert any("bad.json" in r.message for r in caplog.records)
    assert len(browser._all_puzzles) == 2
