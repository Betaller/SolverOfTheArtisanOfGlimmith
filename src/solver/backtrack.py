from __future__ import annotations

import time
import threading
from collections import deque

from src.models.board import Board
from src.models.puzzle import Puzzle
from src.models.solution import Solution
from src.solver.exceptions import NoSolutionError, SolverTimeoutError
from src.solver.propagator import ConstraintPropagator, update_boundary_edges
from src.solver.validator import SolutionValidator
from src.solver.constraints import _rose_symbol_types, _rose_M

from src.solver.candidates import (
    _get_complete_area, _has_fixed_area, _target_areas,
    _get_component, _generate_region_candidates,
    _max_region_area, _min_region_area, _frontier,
    _enumerate_regions, _region_feasible,
    _check_compass_dir, _shape_matches, _is_rectangle_shape,
    _count_components, _get_all_components, _has_internal_boundary,
    _boundary_graph_is_bipartite,
)

from src.solver.checks import (
    _check_incremental, _get_adjacent_region_ids,
    _check_global_constraints, _check_compass_final,
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
        if self.puzzle.has_rule("rose_window"):
            result = self._solve_rose_growth(self._board, all_positions)
            if result is not None:
                from src.solver.constraints import check_boundary_consistency
                if check_boundary_consistency(self._board):
                    self.steps = 1
                    update_boundary_edges(self._board)
                    solution = self.validator.validate(self.puzzle, self._board)
                    solution.steps_taken = self.steps
                    solution.elapsed_ms = int((time.monotonic() - self.start_time) * 1000)
                    if solution.solved:
                        return solution
            self._board = self._board_from_puzzle()

            symbol_types = _rose_symbol_types(self.puzzle, self._board)
            if symbol_types:
                first_type = symbol_types[0]
                seeds = [(r, c) for r in range(self._board.height) for c in range(self._board.width)
                         if self._board.cell(r, c).symbol == first_type and not self._board.cell(r, c).blocked]
                n_seeds = len(seeds)
                if n_seeds > 1:
                    per_seed = min(30.0, max(1.0, self.timeout / n_seeds))
                    for seed_idx, seed in enumerate(seeds):
                        elapsed = time.monotonic() - self.start_time
                        if elapsed > self.timeout:
                            break
                        sub = BacktrackSolver(self.puzzle)
                        sub_board = sub._board_from_puzzle()
                        rs, cs = seed
                        sub_board.cell(rs, cs).region_id = 0
                        sub_unassigned = all_positions - {seed}
                        sub_regions = {0: {seed}}
                        sub.start_time = time.monotonic()
                        sub.timeout = min(per_seed, self.timeout - elapsed)
                        sub.steps = 0
                        sub._first_shape_key = None
                        sub_result = sub._search(sub_board, sub_unassigned, sub_regions, 1)
                        if sub_result is not None:
                            for r in range(board.height):
                                for c in range(board.width):
                                    board.cell(r, c).region_id = sub_board.cell(r, c).region_id
                            self.steps += sub.steps
                            update_boundary_edges(self._board)
                            solution = self.validator.validate(self.puzzle, self._board)
                            solution.steps_taken = self.steps
                            solution.elapsed_ms = int((time.monotonic() - self.start_time) * 1000)
                            if solution.solved:
                                return solution
                    self._board = self._board_from_puzzle()
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
        self._pre_boundaries = set()
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

    def _search(self, board: Board, unassigned: set[tuple[int, int]],
                regions: dict[int, set[tuple[int, int]]], next_rid: int) -> dict[int, set[tuple[int, int]]] | None:
        if not unassigned:
            if self._check_global_constraints(board, regions):
                return regions
            return None

        elapsed = time.monotonic() - self.start_time
        if elapsed > self.timeout:
            return None

        self.steps += 1

        seed = self._pick_seed(unassigned)

        if self.puzzle.has_rule("rose_window"):
            M = _rose_M(self.puzzle, board)
            if M > 0 and len(regions) >= M:
                return None

        candidates = self._generate_region_candidates(board, seed, unassigned)

        if self.puzzle.has_rule("rose_window"):
            M = _rose_M(self.puzzle, board)
            if M > 0 and len(regions) == M - 1:
                last_component = self._get_component(board, seed, unassigned)
                if self._region_feasible(board, last_component):
                    candidates = [last_component]

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

            if not self._rose_capacity_ok(board, new_unassigned):
                self._unassign(board, region_cells)
                continue

            result = self._search(board, new_unassigned, new_regions, next_rid + 1)
            if result is not None:
                return result

            self._unassign(board, region_cells)

        return None

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

    def _rose_capacity_ok(self, board: Board, unassigned: set[tuple[int, int]]) -> bool:
        if not self.puzzle.has_rule("rose_window"):
            return True
        M = _rose_M(self.puzzle, board)
        if M <= 0:
            return True
        assigned_ids: set[int] = set()
        for r in range(board.height):
            for c in range(board.width):
                rid = board.cell(r, c).region_id
                if rid is not None:
                    assigned_ids.add(rid)
        remaining_regions = M - len(assigned_ids)
        if remaining_regions <= 0:
            return len(unassigned) == 0
        remaining_cells = len(unassigned)
        if remaining_cells < remaining_regions:
            return False
        comp_count = _count_components(unassigned, board, self._pre_boundaries)
        if remaining_regions <= 2 and comp_count > remaining_regions:
            return False
        if comp_count == 1:
            comps = _get_all_components(unassigned, board, self._pre_boundaries)
            if comps and _has_internal_boundary(comps[0], self._pre_boundaries):
                if remaining_regions == 1:
                    return False
                if remaining_regions == 2:
                    if not _boundary_graph_is_bipartite(comps[0], self._pre_boundaries):
                        return False
        return True

    def _pick_seed(self, unassigned: set[tuple[int, int]]) -> tuple[int, int]:
        if self.puzzle.has_rule("area") and hasattr(self, '_board'):
            clue_seeds = [p for p in unassigned if self._board.cell(p[0], p[1]).number is not None]
            if clue_seeds:
                clue_seeds.sort(key=lambda p: self._board.cell(p[0], p[1]).number)
                return clue_seeds[0]
        if self.puzzle.has_rule("rose_window") and hasattr(self, '_board'):
            sym_seeds = [p for p in unassigned if self._board.cell(p[0], p[1]).symbol is not None]
            if sym_seeds:
                return min(sym_seeds)
        return min(unassigned)

    def _solve_rose_growth(self, board: Board,
                           all_positions: set[tuple[int, int]]) -> dict[int, set[tuple[int, int]]] | None:
        symbol_types = _rose_symbol_types(self.puzzle, board)
        if not symbol_types:
            return None
        M = _rose_M(self.puzzle, board)
        if M <= 0:
            return None

        first_type = symbol_types[0]
        seeds = [(r, c) for r in range(board.height) for c in range(board.width)
                 if board.cell(r, c).symbol == first_type and not board.cell(r, c).blocked]
        if len(seeds) != M:
            return None

        boundary_endpoints: set[tuple[int, int]] = set()
        for r1, c1, r2, c2 in self._pre_boundaries:
            boundary_endpoints.add((r1, c1))
            boundary_endpoints.add((r2, c2))

        region_symbols: list[set[str]] = [{first_type} for _ in range(M)]
        region_sizes: list[int] = [1] * M

        for i, (r, c) in enumerate(seeds):
            board.cell(r, c).region_id = i

        queue: deque[tuple[int, int, int, int]] = deque()
        for i, (r, c) in enumerate(seeds):
            queue.append((r, c, i, 0))

        while queue:
            r, c, rid, dist = queue.popleft()
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if not (0 <= nr < board.height and 0 <= nc < board.width):
                    continue
                if board.cell(nr, nc).blocked:
                    continue
                if board.cell(nr, nc).region_id is not None:
                    continue
                key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
                if key in self._pre_boundaries:
                    continue

                sym = board.cell(nr, nc).symbol
                if sym is not None and sym in region_symbols[rid]:
                    continue
                if (nr, nc) in boundary_endpoints:
                    in_same = False
                    for ddr, ddc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                        nnr, nnc = nr + ddr, nc + ddc
                        if 0 <= nnr < board.height and 0 <= nnc < board.width:
                            if board.cell(nnr, nnc).region_id == rid:
                                k2 = (nr, nc, nnr, nnc)
                                if nr < nnr or (nr == nnr and nc < nnc):
                                    k2 = (nr, nc, nnr, nnc)
                                else:
                                    k2 = (nnr, nnc, nr, nc)
                                if k2 in self._pre_boundaries:
                                    in_same = True
                                    break
                    if in_same:
                        continue

                board.cell(nr, nc).region_id = rid
                if sym is not None:
                    region_symbols[rid].add(sym)
                region_sizes[rid] += 1
                queue.append((nr, nc, rid, dist + 1))

        unassigned = {(r, c) for r in range(board.height) for c in range(board.width)
                     if board.cell(r, c).region_id is None and not board.cell(r, c).blocked}
        if unassigned:
            changed = True
            while changed:
                changed = False
                for r, c in list(unassigned):
                    candidates: set[int] = set()
                    for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                        nr, nc = r + dr, c + dc
                        if 0 <= nr < board.height and 0 <= nc < board.width:
                            nrid = board.cell(nr, nc).region_id
                            if nrid is not None:
                                key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
                                if key not in self._pre_boundaries:
                                    candidates.add(nrid)
                    if candidates:
                        sym = board.cell(r, c).symbol
                        valid = [i for i in candidates
                                 if not (sym is not None and sym in region_symbols[i])]
                        if sym is not None and len(valid) < len(candidates):
                            pass
                        if valid:
                            ok_valid = []
                            for i in valid:
                                bad = False
                                for ddr, ddc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                                    nnr, nnc = r + ddr, c + ddc
                                    if 0 <= nnr < board.height and 0 <= nnc < board.width:
                                        if board.cell(nnr, nnc).region_id == i:
                                            k2 = (r, c, nnr, nnc) if r < nnr or (r == nnr and c < nnc) else (nnr, nnc, r, c)
                                            if k2 in self._pre_boundaries:
                                                bad = True
                                                break
                                if not bad:
                                    ok_valid.append(i)
                            if ok_valid:
                                best = min(ok_valid, key=lambda i: region_sizes[i])
                                board.cell(r, c).region_id = best
                                if sym is not None:
                                    region_symbols[best].add(sym)
                                region_sizes[best] += 1
                                unassigned.discard((r, c))
                                changed = True
            if unassigned:
                return None

        if not all(len(syms) == len(symbol_types) for syms in region_symbols):
            return None

        regions = {}
        for r in range(board.height):
            for c in range(board.width):
                rid = board.cell(r, c).region_id
                if rid is not None:
                    regions.setdefault(rid, set()).add((r, c))
        return regions

    def _solve_rose_parallel(self, board: Board,
                              all_positions: set[tuple[int, int]],
                              seeds: list[tuple[int, int]]) -> dict[int, set[tuple[int, int]]] | None:
        n = len(seeds)
        per_seed_timeout = min(10.0, max(1.0, self.timeout / n))
        result_container: list[dict[int, set[tuple[int, int]]] | None] = [None]

        def _try_seed(seed_idx: int) -> None:
            seed = seeds[seed_idx]
            sub = BacktrackSolver(self.puzzle)
            sub_board = sub._board_from_puzzle()
            rs, cs = seed
            sub_board.cell(rs, cs).region_id = 0
            sub_unassigned = all_positions - {seed}
            sub_regions = {0: {seed}}
            sub.start_time = time.monotonic()
            sub.timeout = per_seed_timeout
            sub.steps = 0
            sub._first_shape_key = None
            sub_result = sub._search(sub_board, sub_unassigned, sub_regions, 1)
            if sub_result is not None and result_container[0] is None:
                result_container[0] = sub_result
                for r in range(board.height):
                    for c in range(board.width):
                        dst = board.cell(r, c)
                        src = sub_board.cell(r, c)
                        dst.region_id = src.region_id
                self.steps += sub.steps

        threads = []
        for i in range(n):
            t = threading.Thread(target=_try_seed, args=(i,), daemon=True)
            threads.append(t)
            t.start()
        join_timeout = per_seed_timeout + 2.0
        for t in threads:
            t.join(timeout=join_timeout)
        return result_container[0]

from src.solver.candidates import (
    _get_complete_area as _get_complete_area_fn,
    _has_fixed_area as _has_fixed_area_fn,
    _target_areas as _target_areas_fn,
    _get_component as _get_component_fn,
    _generate_region_candidates as _generate_region_candidates_fn,
    _max_region_area as _max_region_area_fn,
    _min_region_area as _min_region_area_fn,
    _frontier as _frontier_fn,
    _enumerate_regions as _enumerate_regions_fn,
    _region_feasible as _region_feasible_fn,
    _check_compass_dir as _check_compass_dir_fn,
    _shape_matches as _shape_matches_fn,
    _is_rectangle_shape as _is_rectangle_shape_fn,
)
from src.solver.checks import (
    _check_incremental as _check_incremental_fn,
    _get_adjacent_region_ids as _get_adjacent_region_ids_fn,
    _check_global_constraints as _check_global_constraints_fn,
    _check_compass_final as _check_compass_final_fn,
)

BacktrackSolver._get_complete_area = _get_complete_area_fn
BacktrackSolver._has_fixed_area = _has_fixed_area_fn
BacktrackSolver._target_areas = _target_areas_fn
BacktrackSolver._get_component = _get_component_fn
BacktrackSolver._generate_region_candidates = _generate_region_candidates_fn
BacktrackSolver._max_region_area = _max_region_area_fn
BacktrackSolver._min_region_area = _min_region_area_fn
BacktrackSolver._frontier = _frontier_fn
BacktrackSolver._enumerate_regions = _enumerate_regions_fn
BacktrackSolver._region_feasible = _region_feasible_fn
BacktrackSolver._check_compass_dir = _check_compass_dir_fn
BacktrackSolver._shape_matches = _shape_matches_fn
BacktrackSolver._is_rectangle_shape = _is_rectangle_shape_fn
BacktrackSolver._check_incremental = _check_incremental_fn
BacktrackSolver._get_adjacent_region_ids = _get_adjacent_region_ids_fn
BacktrackSolver._check_global_constraints = _check_global_constraints_fn
BacktrackSolver._check_compass_final = _check_compass_final_fn
