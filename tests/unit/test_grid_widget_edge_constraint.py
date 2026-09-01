"""Regression tests for the edge-constraint handler in GridWidget.

`src/ui/grid_widget.py` constructs `EdgeConstraint(...)` in `_set_edge_constraint`
(reachable from the edge context menu), but its import from `src.models.board`
only brought in `EdgeConstraintType`.  Every edge-constraint menu action
("设异生/设双生/设不等号/设差值") therefore raised `NameError` at runtime.
These tests drive that handler so the crash is caught in CI.
"""
from __future__ import annotations

import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import pytest
from PySide6.QtWidgets import QApplication

from src.models.board import Board, EdgeConstraint, EdgeConstraintType


@pytest.fixture(scope="session", autouse=True)
def qapp():
    """Ensure a QApplication exists (offscreen) for every test in this module."""
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _widget():
    from src.ui.grid_widget import GridWidget

    w = GridWidget()
    w.set_board(Board(3, 3))
    return w


def test_grid_widget_has_edge_constraint_in_scope() -> None:
    """Guard against the missing import (F821) coming back."""
    import src.ui.grid_widget as gw

    assert gw.EdgeConstraint is EdgeConstraint


def test_set_edge_constraint_heterogeneous() -> None:
    w = _widget()
    edge = w.board.edge_between(0, 0, 0, 1)
    assert edge is not None
    assert edge.constraint is None

    w._set_edge_constraint(0, 0, 0, 1, EdgeConstraintType.HETEROGENEOUS)

    assert isinstance(edge.constraint, EdgeConstraint)
    assert edge.constraint.type == EdgeConstraintType.HETEROGENEOUS
    assert edge.constraint.value is None


def test_set_edge_constraint_stores_value() -> None:
    w = _widget()
    edge = w.board.edge_between(1, 0, 1, 1)

    w._set_edge_constraint(1, 0, 1, 1, EdgeConstraintType.DIFFERENCE, 1)

    assert edge.constraint is not None
    assert edge.constraint.type == EdgeConstraintType.DIFFERENCE
    assert edge.constraint.value == 1


def test_set_and_clear_edge_constraint_inequality() -> None:
    w = _widget()
    edge = w.board.edge_between(0, 1, 1, 1)

    w._set_edge_constraint(0, 1, 1, 1, EdgeConstraintType.INEQUALITY)
    assert edge.constraint is not None
    assert edge.constraint.type == EdgeConstraintType.INEQUALITY

    w._clear_edge_constraint(0, 1, 1, 1)
    assert edge.constraint is None


def test_set_edge_constraint_homogeneous() -> None:
    w = _widget()
    edge = w.board.edge_between(2, 0, 2, 1)

    w._set_edge_constraint(2, 0, 2, 1, EdgeConstraintType.HOMOGENEOUS)

    assert edge.constraint is not None
    assert edge.constraint.type == EdgeConstraintType.HOMOGENEOUS
