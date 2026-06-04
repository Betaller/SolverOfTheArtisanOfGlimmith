from __future__ import annotations

from src.models.board import Board, Shape
from src.models.puzzle import Puzzle
from src.models.solution import RegionInfo, Solution
from src.solver.constraints import (
    RULE_CHECKERS,
    get_region_cells,
    get_region_shape,
    check_region_connectivity,
    check_boundary_consistency,
)
from src.solver.shapes import canonical_key, match_shape_pool


class SolutionValidator:
    def validate(self, puzzle: Puzzle, board: Board) -> Solution:
        regions = get_region_cells(board)
        region_infos: list[RegionInfo] = []
        for rid, cells in regions.items():
            shape = get_region_shape(cells)
            key = canonical_key(shape.cells)
            pool_name = None
            pool_rule = puzzle.get_rule("shape_pool")
            if pool_rule is not None:
                pool_shapes = pool_rule.params.get("shapes", [])
                pool_name = match_shape_pool(shape, pool_shapes)
            region_infos.append(RegionInfo(
                region_id=rid,
                cells=[(c.row, c.col) for c in cells],
                area=len(cells),
                shape=shape,
                normalized_shape_key=key,
                matched_shape_name=pool_name,
            ))
        
        errors: dict[str, str] = {}
        
        if not check_region_connectivity(board):
            errors["connectivity"] = "区域不满足四连通"
        
        if not check_boundary_consistency(board):
            errors["boundary"] = "边框划分与区域不一致"
        
        if puzzle.rules:
            for rule in puzzle.rules:
                checker = RULE_CHECKERS.get(rule.type)
                if checker is not None:
                    if not checker(puzzle, board):
                        errors[rule.type] = f"规则 '{rule.display_name}' 验证失败"
        
        solved = len(errors) == 0 and all(
            c.region_id is not None for c in board.cells() if not c.blocked
        )
        
        return Solution(
            board=board,
            solved=solved,
            regions=region_infos,
            rule_results={k: k not in errors for k in RULE_CHECKERS},
            error_message="; ".join(errors.values()) if errors else None,
        )


def validate_solution(puzzle: Puzzle, board: Board) -> Solution:
    validator = SolutionValidator()
    return validator.validate(puzzle, board)
