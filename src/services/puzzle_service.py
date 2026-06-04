from __future__ import annotations

import os
from typing import Optional

from src.models.board import Board
from src.models.puzzle import Puzzle, Rule, check_rule_conflicts
from src.solver.exceptions import PuzzleFormatError
from src.io.puzzle_codec import serialize, deserialize


class PuzzleService:
    def create_puzzle(self, height: int, width: int) -> Puzzle:
        board = Board(height, width)
        return Puzzle.from_board(board)

    def load_puzzle(self, path: str) -> Puzzle:
        if not os.path.exists(path):
            raise PuzzleFormatError(f"文件不存在: {path}")
        return deserialize(path)

    def save_puzzle(self, puzzle: Puzzle, path: str) -> None:
        serialize(puzzle, path)

    def add_rule(self, puzzle: Puzzle, rule: Rule) -> list[str]:
        puzzle.rules.append(rule)
        return check_rule_conflicts(puzzle.rules)

    def remove_rule(self, puzzle: Puzzle, rule_type: str) -> None:
        puzzle.rules = [r for r in puzzle.rules if r.type != rule_type]

    def validate_rules(self, puzzle: Puzzle) -> list[str]:
        return check_rule_conflicts(puzzle.rules)

    def get_puzzle_info(self, puzzle: Puzzle) -> dict:
        return {
            "size": f"{puzzle.height}x{puzzle.width}",
            "cells": len(puzzle.cells),
            "rules": [r.display_name for r in puzzle.rules],
            "has_conflicts": len(check_rule_conflicts(puzzle.rules)) > 0,
        }
