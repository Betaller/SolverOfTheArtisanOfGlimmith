from __future__ import annotations

from src.io.puzzle_codec import _canonical_outer, dict_to_puzzle


class TestCanonicalOuter:
    def test_top_segment_unchanged(self) -> None:
        assert _canonical_outer((0, 2, 0, 3)) == (0, 2, 0, 3)

    def test_bottom_segment_unchanged(self) -> None:
        assert _canonical_outer((4, 1, 4, 2)) == (4, 1, 4, 2)

    def test_left_segment_unchanged(self) -> None:
        assert _canonical_outer((1, 0, 2, 0)) == (1, 0, 2, 0)

    def test_reversed_horizontal_endpoints(self) -> None:
        assert _canonical_outer((0, 3, 0, 2)) == (0, 2, 0, 3)

    def test_reversed_vertical_endpoints(self) -> None:
        assert _canonical_outer((2, 0, 1, 0)) == (1, 0, 2, 0)

    def test_non_adjacent_segment_rejected(self) -> None:
        assert _canonical_outer((0, 0, 0, 3)) is None

    def test_diagonal_segment_rejected(self) -> None:
        assert _canonical_outer((0, 0, 1, 1)) is None

    def test_zero_length_segment_rejected(self) -> None:
        assert _canonical_outer((0, 0, 0, 0)) is None


class TestDictToPuzzleOuterBoundaries:
    def _minimal(self, outer: list[list[int]]) -> dict:
        return {
            "version": "1.0",
            "grid": {"height": 4, "width": 4},
            "cells": [],
            "edges": [],
            "vertices": [],
            "outer_boundaries": [
                {"r1": r1, "c1": c1, "r2": r2, "c2": c2}
                for r1, c1, r2, c2 in outer
            ],
            "rules": [],
            "shape_pool": [],
        }

    def test_normalizes_reversed_top(self) -> None:
        # stored with reversed endpoints must still load as canonical top segment
        puzzle = dict_to_puzzle(self._minimal([[0, 3, 0, 2]]))
        assert puzzle.outer_boundaries == [(0, 2, 0, 3)]

    def test_keeps_valid_reference_convention(self) -> None:
        puzzle = dict_to_puzzle(self._minimal([[0, 1, 0, 2], [3, 0, 4, 0]]))
        assert puzzle.outer_boundaries == [(0, 1, 0, 2), (3, 0, 4, 0)]

    def test_drops_invalid_segments(self) -> None:
        puzzle = dict_to_puzzle(self._minimal([[0, 0, 0, 0], [0, 0, 1, 1]]))
        assert puzzle.outer_boundaries == []
