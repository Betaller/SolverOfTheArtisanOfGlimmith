from __future__ import annotations

import time
from collections import deque

from src.models.board import Board, Cell, Shape
from src.models.puzzle import Puzzle
from src.models.solution import RegionInfo, Solution
from src.solver.exceptions import NoSolutionError, SolverTimeoutError
from src.solver.propagator import ConstraintPropagator, update_boundary_edges
from src.solver.shapes import canonical_key, match_shape_pool
from src.solver.validator import SolutionValidator
from src.solver.constraints import (
    check_rule_same, check_rule_different, check_rule_mixed,
    check_rule_heterogeneous, check_rule_homogeneous,
    check_rule_differentiation, check_rule_brick, check_rule_ring,
    check_rule_inequality, check_rule_difference,
    check_rule_watchtower, check_rule_fence, check_rule_compass,
    check_rule_rose_window, check_rule_shape_pool,
    check_rule_area, check_rule_precise, check_rule_range,
    check_rule_solitary, check_rule_block, check_rule_non_block,
    check_rule_puzzle_piece,
)


class BacktrackSolver:
    def __init__(self, puzzle: Puzzle) -> None:
        self.puzzle = puzzle
        self.propagator = ConstraintPropagator(puzzle)
        self.validator = SolutionValidator()
        self.steps = 0
        self.start_time = 0.0
        self.timeout = 30.0
        self._first_shape_key: str | None = None
        self._pre_boundaries: set[tuple[int, int, int, int]] = set()
        self._pre_boundaries_blocking: bool = False

    def solve(self, timeout: float = 30.0) -> Solution:
        self.timeout = timeout
        self.start_time = time.monotonic()
        self.steps = 0
        self._first_shape_key = None

        self._board = self._board_from_puzzle()

        all_positions = {(r, c) for r in range(self._board.height) for c in range(self._board.width)
                         if not self._board.cell(r, c).blocked}
        if not all_positions:
            update_boundary_edges(self._board)
            solution = self.validator.validate(self.puzzle, self._board)
            solution.steps_taken = self.steps
            solution.elapsed_ms = 0
            return solution
        result = self._search(self._board, all_positions, {}, 0)

        if result is None:
            return Solution(
                board=self._board, solved=False,
                steps_taken=self.steps,
                elapsed_ms=int((time.monotonic() - self.start_time) * 1000),
                error_message="无解",
            )

        update_boundary_edges(self._board)
        solution = self.validator.validate(self.puzzle, self._board)
        solution.steps_taken = self.steps
        solution.elapsed_ms = int((time.monotonic() - self.start_time) * 1000)
        return solution

    def _board_from_puzzle(self) -> Board:
        board = Board(self.puzzle.height, self.puzzle.width)
        for c in self.puzzle.cells:
            dst = board.cell(c.row, c.col)
            dst.number = c.number
            dst.symbol = c.symbol
            dst.shape_pattern = c.shape_pattern
            dst.compass = c.compass
            dst.fence_pattern = c.fence_pattern
            dst.blocked = c.blocked
        self._pre_boundaries: set[tuple[int, int, int, int]] = set()
        self._pre_boundaries_blocking = False
        for e in self.puzzle.edges:
            edge = board.edge_between(e.r1, e.c1, e.r2, e.c2)
            if edge is not None:
                edge.is_boundary = e.is_boundary
                edge.constraint = e.constraint
                if e.is_boundary:
                    r1, c1, r2, c2 = e.r1, e.c1, e.r2, e.c2
                    key = (r1, c1, r2, c2) if r1 < r2 or (r1 == r2 and c1 < c2) else (r2, c2, r1, c1)
                    self._pre_boundaries.add(key)
                    c1_cell = board.cell(r1, c1)
                    c2_cell = board.cell(r2, c2)
                    if not c1_cell.blocked and not c2_cell.blocked:
                        self._pre_boundaries_blocking = True
        for v in self.puzzle.vertices:
            vert = board.vertex_at(v.row, v.col)
            if vert is not None:
                vert.watchtower = v.watchtower
        return board

    def _search(
        self,
        board: Board,
        unassigned: set[tuple[int, int]],
        regions: dict[int, set[tuple[int, int]]],
        next_rid: int,
    ) -> dict[int, set[tuple[int, int]]] | None:
        if not unassigned:
            if self._check_global_constraints(board, regions):
                return regions
            return None

        elapsed = time.monotonic() - self.start_time
        if elapsed > self.timeout:
            return None

        self.steps += 1

        seed = self._pick_seed(unassigned)
        candidates = self._generate_region_candidates(board, seed, unassigned)

        for region_cells in candidates:
            rid = next_rid
            for r, c in region_cells:
                board.cell(r, c).region_id = rid
            new_unassigned = unassigned - region_cells
            new_regions = {**regions, rid: region_cells}

            if not self._check_incremental(board, new_regions, rid):
                self._unassign(board, region_cells)
                continue

            if not self._remaining_capacity_ok(board, new_unassigned):
                self._unassign(board, region_cells)
                continue

            result = self._search(board, new_unassigned, new_regions, next_rid + 1)
            if result is not None:
                return result

            self._unassign(board, region_cells)

        return None

    def _get_complete_area(self) -> int:
        if self.puzzle.has_rule("precise"):
            return self.puzzle.get_rule("precise").params.get("area", 0)
        range_rule = self.puzzle.get_rule("range")
        if range_rule is not None:
            return range_rule.params.get("max", 999)
        return 999

    def _has_fixed_area(self) -> bool:
        if self.puzzle.has_rule("precise"):
            return True
        pool_rule = self.puzzle.get_rule("shape_pool")
        if pool_rule is not None:
            shapes = pool_rule.params.get("shapes", [])
            if shapes and all(s.area == shapes[0].area for s in shapes):
                return True
        return False

    def _target_areas(self) -> set[int]:
        if self.puzzle.has_rule("precise"):
            return {self.puzzle.get_rule("precise").params.get("area", 0)}
        pool_rule = self.puzzle.get_rule("shape_pool")
        if pool_rule is not None:
            return {s.area for s in pool_rule.params.get("shapes", [])}
        return set()

    def _unassign(self, board: Board, cells: set[tuple[int, int]]) -> None:
        for r, c in cells:
            board.cell(r, c).region_id = None

    def _remaining_capacity_ok(self, board: Board, unassigned: set[tuple[int, int]]) -> bool:
        if not self.puzzle.has_rule("area"):
            return True
        remaining = len(unassigned)
        clue_sum = 0
        clue_count = 0
        for r, c in unassigned:
            n = board.cell(r, c).number
            if n is not None:
                clue_sum += n
                clue_count += 1
        if clue_sum > remaining:
            return False
        if clue_count > 0 and clue_count > remaining:
            return False
        return True

    def _pick_seed(self, unassigned: set[tuple[int, int]]) -> tuple[int, int]:
        if self.puzzle.has_rule("area") and hasattr(self, '_board'):
            clue_seeds = [p for p in unassigned if self._board.cell(p[0], p[1]).number is not None]
            if clue_seeds:
                clue_seeds.sort(key=lambda p: self._board.cell(p[0], p[1]).number)
                return clue_seeds[0]
        return min(unassigned)

    def _get_component(
        self,
        board: Board,
        seed: tuple[int, int],
        unassigned: set[tuple[int, int]],
    ) -> set[tuple[int, int]]:
        if not self._pre_boundaries:
            return set(unassigned)
        component: set[tuple[int, int]] = {seed}
        queue: deque[tuple[int, int]] = deque([seed])
        while queue:
            r, c = queue.popleft()
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if (nr, nc) in unassigned and (nr, nc) not in component:
                    key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
                    if key not in self._pre_boundaries:
                        component.add((nr, nc))
                        queue.append((nr, nc))
        return component

    def _generate_region_candidates(
        self,
        board: Board,
        seed: tuple[int, int],
        unassigned: set[tuple[int, int]],
    ) -> list[set[tuple[int, int]]]:
        results: list[set[tuple[int, int]]] = []

        if self.puzzle.has_rule("shape_pool"):
            pool_rule = self.puzzle.get_rule("shape_pool")
            if pool_rule is not None:
                pool_shapes = pool_rule.params.get("shapes", [])
                if pool_shapes:
                    component = self._get_component(board, seed, unassigned)
                    from src.solver.shapes import all_transformations
                    seen: set[frozenset] = set()
                    sr, sc = seed
                    for ps in pool_shapes:
                        for tf in all_transformations(ps.cells):
                            for rs, cs in tf:
                                dr = sr - rs
                                dc = sc - cs
                                placed_fs = frozenset((r + dr, c + dc) for (r, c) in tf)
                                if placed_fs in seen:
                                    continue
                                seen.add(placed_fs)
                                if component:
                                    if any((r, c) not in component for (r, c) in placed_fs):
                                        continue
                                else:
                                    if any((r, c) not in unassigned for (r, c) in placed_fs):
                                        continue
                                placed = set(placed_fs)
                                if not self._region_feasible(board, placed):
                                    continue
                                results.append(placed)
                    if results:
                        results.sort(key=lambda s: len(s), reverse=True)
                        return results

        seed_clue = board.cell(seed[0], seed[1]).number
        clue_target = seed_clue if (seed_clue is not None and self.puzzle.has_rule("area")) else None
        if clue_target is not None:
            max_area = clue_target
        else:
            target_areas = self._target_areas()
            if target_areas:
                max_area = max(target_areas)
            else:
                max_area = self._max_region_area()

        initial: set[tuple[int, int]] = {seed}
        frontier = self._frontier(initial, unassigned)

        self._enumerate_regions(board, initial, frontier, unassigned, max_area, results, seed_clue=clue_target)

        if not results:
            results = [{seed}]

        if self.puzzle.has_rule("same"):
            results.sort(key=lambda s: len(s))
        else:
            results.sort(key=lambda s: len(s), reverse=True)
        return results

    def _max_region_area(self) -> int:
        total = self.puzzle.height * self.puzzle.width
        if self.puzzle.has_rule("precise"):
            return self.puzzle.get_rule("precise").params.get("area", total)
        range_rule = self.puzzle.get_rule("range")
        if range_rule is not None:
            return range_rule.params.get("max", total)
        return total

    def _min_region_area(self) -> int:
        range_rule = self.puzzle.get_rule("range")
        if range_rule is not None:
            return range_rule.params.get("min", 1)
        return 1

    def _frontier(
        self,
        region: set[tuple[int, int]],
        unassigned: set[tuple[int, int]],
    ) -> set[tuple[int, int]]:
        result: set[tuple[int, int]] = set()
        for r, c in region:
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if (nr, nc) in unassigned and (nr, nc) not in region:
                    result.add((nr, nc))
        return result

    def _enumerate_regions(
        self,
        board: Board,
        current: set[tuple[int, int]],
        frontier: set[tuple[int, int]],
        unassigned: set[tuple[int, int]],
        max_area: int,
        results: list[set[tuple[int, int]]],
        seed_clue: int | None = None,
    ) -> None:
        if len(current) > max_area:
            return

        if seed_clue is not None:
            if len(current) == seed_clue:
                results.append(set(current))
            elif len(current) > max_area:
                return
        else:
            target_areas = self._target_areas()
            if target_areas:
                if len(current) in target_areas:
                    results.append(set(current))
                elif len(current) > max(target_areas):
                    return
            elif len(current) >= self._min_region_area():
                results.append(set(current))

        if len(current) >= max_area:
            return

        if len(results) >= 200:
            return

        frontier_list = sorted(frontier)
        for i, cell in enumerate(frontier_list):
            new_region = current | {cell}
            new_frontier = (frontier - {cell}) | self._frontier({cell}, unassigned - new_region)

            if not self._region_feasible(board, new_region):
                continue

            self._enumerate_regions(board, new_region, new_frontier, unassigned, max_area, results, seed_clue)

    def _region_feasible(self, board: Board, cells: set[tuple[int, int]]) -> bool:
        area = len(cells)

        if self.puzzle.has_rule("precise"):
            target = self.puzzle.get_rule("precise").params.get("area", 0)
            if area > target:
                return False

        range_rule = self.puzzle.get_rule("range")
        if range_rule is not None:
            max_a = range_rule.params.get("max", 999)
            if area > max_a:
                return False

        clue_values = set()
        for r, c in cells:
            cell = board.cell(r, c)
            if cell.number is not None and self.puzzle.has_rule("area"):
                clue_values.add(cell.number)
                if cell.number < area:
                    return False

        if len(clue_values) > 1:
            return False

        for r, c in cells:
            cell = board.cell(r, c)
            if cell.compass is not None and self.puzzle.has_rule("compass"):
                rid_placeholder = -1
                for dr, dc, attr in [(-1, 0, "up"), (1, 0, "down"), (0, -1, "left"), (0, 1, "right")]:
                    expected = getattr(cell.compass, attr)
                    if expected == -1:
                        continue
                    if not self._check_compass_dir(board, r, c, dr, dc, expected, cells):
                        return False

        if self.puzzle.has_rule("solitary"):
            symbols = [board.cell(r, c).symbol for r, c in cells if board.cell(r, c).symbol is not None]
            if len(symbols) > 1:
                return False

        if self.puzzle.has_rule("precise") or self.puzzle.has_rule("range") or self.puzzle.has_rule("shape_pool") or self.puzzle.has_rule("puzzle_piece"):
            target_area = self._get_complete_area()
            check_shape = len(cells) == target_area
            if not check_shape and self.puzzle.has_rule("shape_pool"):
                pool_rule = self.puzzle.get_rule("shape_pool")
                if pool_rule is not None:
                    pool_shapes = pool_rule.params.get("shapes", [])
                    if pool_shapes and len(cells) in {s.area for s in pool_shapes}:
                        check_shape = True
            if check_shape:
                shape = Shape(cells=frozenset(cells))
                pool_rule = self.puzzle.get_rule("shape_pool")
                if pool_rule is not None:
                    pool_shapes = pool_rule.params.get("shapes", [])
                    if match_shape_pool(shape, pool_shapes) is None:
                        return False

                if self.puzzle.has_rule("puzzle_piece"):
                    for r, c in cells:
                        cell = board.cell(r, c)
                        if cell.shape_pattern is not None:
                            if not self._shape_matches(shape, cell.shape_pattern):
                                return False

        if self.puzzle.has_rule("block"):
            if not self._is_rectangle_shape(cells):
                return False

        if self.puzzle.has_rule("non_block"):
            if self._is_rectangle_shape(cells):
                return False

        if self.puzzle.has_rule("shape_pool"):
            pool_rule = self.puzzle.get_rule("shape_pool")
            if pool_rule is not None:
                pool_shapes = pool_rule.params.get("shapes", [])
                if pool_shapes and not hasattr(self, '_pool_max_extent'):
                    from src.solver.shapes import all_transformations
                    mh = mw = 0
                    for ps in pool_shapes:
                        for tf in all_transformations(ps.cells):
                            rs = [r for r, _ in tf]
                            cs = [c for _, c in tf]
                            mh = max(mh, max(rs) - min(rs) + 1)
                            mw = max(mw, max(cs) - min(cs) + 1)
                    self._pool_max_extent = (mh, mw)
                if pool_shapes:
                    min_r = min(r for r, _ in cells)
                    max_r = max(r for r, _ in cells)
                    min_c = min(c for _, c in cells)
                    max_c = max(c for _, c in cells)
                    h = max_r - min_r + 1
                    w = max_c - min_c + 1
                    mh, mw = self._pool_max_extent
                    if h > mh or w > mw:
                        return False

        if self._pre_boundaries:
            for r, c in cells:
                for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                    nr, nc = r + dr, c + dc
                    if (nr, nc) in cells:
                        key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
                        if key in self._pre_boundaries:
                            return False

        return True

    def _check_compass_dir(
        self,
        board: Board,
        r: int, c: int,
        dr: int, dc: int,
        expected: int,
        region_cells: set[tuple[int, int]],
    ) -> bool:
        count = 0
        cr, cc = r + dr, c + dc
        while 0 <= cr < board.height and 0 <= cc < board.width:
            if (cr, cc) in region_cells:
                count += 1
            else:
                break
            cr += dr
            cc += dc
        return count <= expected

    def _shape_matches(self, shape: Shape, pattern: Shape) -> bool:
        from src.solver.shapes import shapes_equal
        return shapes_equal(shape, pattern)

    def _is_rectangle_shape(self, cells: set[tuple[int, int]]) -> bool:
        if not cells:
            return False
        min_r = min(r for r, _ in cells)
        max_r = max(r for r, _ in cells)
        min_c = min(c for _, c in cells)
        max_c = max(c for _, c in cells)
        return len(cells) == (max_r - min_r + 1) * (max_c - min_c + 1)

    def _check_incremental(
        self,
        board: Board,
        regions: dict[int, set[tuple[int, int]]],
        new_rid: int,
    ) -> bool:
        new_cells = regions[new_rid]

        if self.puzzle.has_rule("solitary"):
            symbols = [board.cell(r, c).symbol for r, c in new_cells if board.cell(r, c).symbol is not None]
            if len(symbols) > 1:
                return False

        if self.puzzle.has_rule("precise"):
            target = self.puzzle.get_rule("precise").params.get("area", 0)
            if len(new_cells) != target:
                return False

        if self.puzzle.has_rule("area"):
            for r, c in new_cells:
                cell = board.cell(r, c)
                if cell.number is not None and len(new_cells) < cell.number:
                    return False

        if self.puzzle.has_rule("shape_pool"):
            pool_rule = self.puzzle.get_rule("shape_pool")
            if pool_rule is not None:
                pool_shapes = pool_rule.params.get("shapes", [])
                if pool_shapes and len(new_cells) not in {s.area for s in pool_shapes}:
                    return False

        new_shape = Shape(cells=frozenset(new_cells))
        new_shape_key = canonical_key(new_shape.cells)

        if self.puzzle.has_rule("same"):
            if self._first_shape_key is None:
                self._first_shape_key = new_shape_key
            elif new_shape_key != self._first_shape_key:
                return False

        if self.puzzle.has_rule("different"):
            for rid, cells in regions.items():
                if rid == new_rid:
                    continue
                other_shape = Shape(cells=frozenset(cells))
                if canonical_key(other_shape.cells) == new_shape_key:
                    return False

        if self.puzzle.has_rule("mixed"):
            for other_rid in self._get_adjacent_region_ids(board, new_rid, regions):
                other_cells = regions[other_rid]
                other_shape = Shape(cells=frozenset(other_cells))
                if canonical_key(other_shape.cells) == new_shape_key:
                    return False

        if self.puzzle.has_rule("heterogeneous") or self.puzzle.has_rule("homogeneous"):
            from src.models.board import EdgeConstraintType
            for e in board.edges():
                if e.constraint is None:
                    continue
                c1 = board.cell(e.r1, e.c1)
                c2 = board.cell(e.r2, e.c2)
                if not c1.assigned or not c2.assigned:
                    continue
                if c1.region_id == c2.region_id:
                    continue
                rid1, rid2 = c1.region_id, c2.region_id
                if rid1 == new_rid or rid2 == new_rid:
                    cells1 = regions[rid1]
                    cells2 = regions[rid2]
                    s1 = Shape(cells=frozenset(cells1))
                    s2 = Shape(cells=frozenset(cells2))
                    eq = canonical_key(s1.cells) == canonical_key(s2.cells)
                    if e.constraint.type == EdgeConstraintType.HETEROGENEOUS and eq:
                        return False
                    if e.constraint.type == EdgeConstraintType.HOMOGENEOUS and not eq:
                        return False

        if self.puzzle.has_rule("differentiation"):
            for other_rid in self._get_adjacent_region_ids(board, new_rid, regions):
                if len(regions[other_rid]) == len(new_cells):
                    return False

        if self._pre_boundaries:
            for r, c in new_cells:
                for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                    nr, nc = r + dr, c + dc
                    if 0 <= nr < board.height and 0 <= nc < board.width:
                        neighbor = board.cell(nr, nc)
                        if neighbor.assigned and neighbor.region_id == new_rid:
                            key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
                            if key in self._pre_boundaries:
                                return False

        return True

    def _get_adjacent_region_ids(
        self,
        board: Board,
        rid: int,
        regions: dict[int, set[tuple[int, int]]],
    ) -> set[int]:
        result: set[int] = set()
        for r, c in regions[rid]:
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if 0 <= nr < board.height and 0 <= nc < board.width:
                    neighbor = board.cell(nr, nc)
                    if neighbor.assigned and neighbor.region_id != rid:
                        result.add(neighbor.region_id)
        return result

    def _check_global_constraints(
        self,
        board: Board,
        regions: dict[int, set[tuple[int, int]]],
    ) -> bool:
        if not regions:
            return False

        for rid, cells in regions.items():
            for r, c in cells:
                cell = board.cell(r, c)
                if cell.compass is not None and self.puzzle.has_rule("compass"):
                    if not self._check_compass_final(board, cell, regions[rid]):
                        return False

        if self.puzzle.has_rule("shape_pool"):
            if not check_rule_shape_pool(self.puzzle, board):
                return False

        if self.puzzle.has_rule("rose_window"):
            if not check_rule_rose_window(self.puzzle, board):
                return False

        if self.puzzle.has_rule("mixed"):
            if not check_rule_mixed(self.puzzle, board):
                return False

        if self.puzzle.has_rule("differentiation"):
            if not check_rule_differentiation(self.puzzle, board):
                return False

        if self.puzzle.has_rule("brick"):
            if not check_rule_brick(self.puzzle, board):
                return False

        if self.puzzle.has_rule("ring"):
            if not check_rule_ring(self.puzzle, board):
                return False

        if self.puzzle.has_rule("inequality"):
            if not check_rule_inequality(self.puzzle, board):
                return False

        if self.puzzle.has_rule("difference"):
            if not check_rule_difference(self.puzzle, board):
                return False

        if self.puzzle.has_rule("watchtower"):
            if not check_rule_watchtower(self.puzzle, board):
                return False

        if self.puzzle.has_rule("fence"):
            if not check_rule_fence(self.puzzle, board):
                return False

        if self.puzzle.has_rule("compass"):
            if not check_rule_compass(self.puzzle, board):
                return False

        if self._pre_boundaries:
            from src.solver.constraints import check_boundary_consistency
            if not check_boundary_consistency(board):
                return False

        return True

    def _check_compass_final(
        self,
        board: Board,
        cell: Cell,
        region_cells: set[tuple[int, int]],
    ) -> bool:
        from dataclasses import dataclass
        r, c = cell.row, cell.col
        for dr, dc, attr in [(-1, 0, "up"), (1, 0, "down"), (0, -1, "left"), (0, 1, "right")]:
            expected = getattr(cell.compass, attr)
            if expected == -1:
                continue
            count = 0
            cr, cc = r + dr, c + dc
            while 0 <= cr < board.height and 0 <= cc < board.width:
                if board.cell(cr, cc).region_id == cell.region_id:
                    count += 1
                else:
                    break
                cr += dr
                cc += dc
            if count != expected:
                return False
        return True
