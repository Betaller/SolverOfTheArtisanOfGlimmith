from __future__ import annotations

import time
import threading
from collections import deque

from src.models.board import Board, Shape
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
    _rose_stop_expanding, _has_size_constraint,
)

from src.solver.checks import (
    _check_incremental, _get_adjacent_region_ids,
    _check_global_constraints, _check_compass_final,
)

from src.solver.shapes import canonical_key, shapes_equal, match_shape_pool

# ── reference: third_party/AoG_Solver (Neptune17, C++) + glimmith-solver (JS) ──


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
            if not self._has_size_constraint():
                result = self._solve_rose_growth(self._board, all_positions)
            else:
                result = None
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
                seeds = sorted((r, c) for r in range(self._board.height)
                               for c in range(self._board.width)
                               if self._board.cell(r, c).symbol == first_type
                               and not self._board.cell(r, c).blocked)
                for seed in seeds:
                    elapsed = time.monotonic() - self.start_time
                    if elapsed > self.timeout:
                        break
                    self._board = self._board_from_puzzle()
                    local_all = {(r, c) for r in range(self._board.height)
                                 for c in range(self._board.width)
                                 if not self._board.cell(r, c).blocked}
                    candidates = self._generate_region_candidates(self._board, seed, local_all)
                    per_candidate_timeout = max(1.0, (self.timeout - elapsed) / max(1, len(candidates)))
                    for region_cells in candidates:
                        elapsed = time.monotonic() - self.start_time
                        if elapsed > self.timeout:
                            break
                        sub = BacktrackSolver(self.puzzle)
                        sub_board = sub._board_from_puzzle()
                        for r, c in region_cells:
                            sub_board.cell(r, c).region_id = 0
                        sub_unassigned = local_all - region_cells
                        sub.start_time = time.monotonic()
                        sub.timeout = min(per_candidate_timeout, self.timeout - elapsed)
                        sub.steps = 0
                        sub._first_shape_key = None
                        sub_result = sub._search(sub_board, sub_unassigned, {0: region_cells}, 1)
                        if sub_result is not None:
                            for rid, cells in sub_result.items():
                                if rid != 0:
                                    for r, c in cells:
                                        sub_board.cell(r, c).region_id = rid
                            for r in range(sub_board.height):
                                for c in range(sub_board.width):
                                    self._board.cell(r, c).region_id = sub_board.cell(r, c).region_id
                            self.steps += sub.steps
                            update_boundary_edges(self._board)
                            solution = self.validator.validate(self.puzzle, self._board)
                            solution.steps_taken = self.steps
                            solution.elapsed_ms = int((time.monotonic() - self.start_time) * 1000)
                            if solution.solved:
                                return solution
                        self.steps += 1
            self._board = self._board_from_puzzle()

        # ── try exact-cover for bounded-candidate puzzles (≈ glimmith-solver / aog DLX) ──
        ec_result = self._try_exact_cover(all_positions)
        if ec_result is not None:
            update_boundary_edges(self._board)
            solution = self.validator.validate(self.puzzle, self._board)
            solution.steps_taken = self.steps
            solution.elapsed_ms = int((time.monotonic() - self.start_time) * 1000)
            if solution.solved:
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
        """Component-level feasibility (inspired by Neptune17 empty_area_check).

        After each region placement, analyzes remaining connected components for:
        - area clue capacity, size range, fixed-size divisibility
        - rose window composition (symbol reachability, region count)
        - compass direction feasibility
        """
        if not unassigned:
            return True

        puzzle = self.puzzle
        pre = self._pre_boundaries
        comps = _get_all_components(unassigned, board, pre)

        for comp in comps:
            max_possible = len(comp)
            if _has_internal_boundary(comp, pre) and not _boundary_graph_is_bipartite(comp, pre):
                max_possible = max_possible * 2 // 3

            if puzzle.has_rule("precise"):
                t = puzzle.get_rule("precise").params.get("area", 0)
                if max_possible < t or len(comp) % t != 0:
                    return False
                if len(comp) == t and _has_internal_boundary(comp, pre):
                    return False

            if puzzle.has_rule("range"):
                mn = puzzle.get_rule("range").params.get("min", 1)
                if max_possible < mn:
                    return False

            if puzzle.has_rule("area"):
                clue_sum = sum(
                    board.cell(r, c).number for r, c in comp
                    if board.cell(r, c).number is not None
                )
                if clue_sum > len(comp):
                    return False

            if puzzle.has_rule("same") and self._first_shape_key is not None:
                # Only check divisibility when we know the target size (after first region)
                if puzzle.has_rule("precise"):
                    target = puzzle.get_rule("precise").params.get("area", 0)
                    if len(comp) % target != 0:
                        return False

            if puzzle.has_rule("compass"):
                for r, c in comp:
                    cell = board.cell(r, c)
                    if cell.compass is not None:
                        for dr, dc, attr in [(-1, 0, "up"), (1, 0, "down"), (0, -1, "left"), (0, 1, "right")]:
                            expected = getattr(cell.compass, attr)
                            if expected == -1:
                                continue
                            cnt = 0
                            cr, cc = r + dr, c + dc
                            while 0 <= cr < board.height and 0 <= cc < board.width:
                                if board.cell(cr, cc).blocked:
                                    break
                                if board.cell(cr, cc).assigned:
                                    break
                                if (cr, cc) in comp:
                                    cnt += 1
                                cr += dr
                                cc += dc
                            if cnt < expected:
                                return False

            if puzzle.has_rule("rose_window"):
                sym_count = sum(1 for (r, c) in comp if board.cell(r, c).symbol is not None)
                rose_s = _rose_symbol_types(puzzle, board)
                if rose_s and len(rose_s) == 1:
                    if sym_count == 0:
                        return False
                    if sym_count == 1 and _has_internal_boundary(comp, pre):
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
        if comp_count > remaining_regions:
            return False
        if comp_count == 1:
            comps = _get_all_components(unassigned, board, self._pre_boundaries)
            if comps and _has_internal_boundary(comps[0], self._pre_boundaries):
                if remaining_regions == 1:
                    return False
                if remaining_regions == 2:
                    if not _boundary_graph_is_bipartite(comps[0], self._pre_boundaries):
                        return False

        rose_symbols = _rose_symbol_types(self.puzzle, board)
        if rose_symbols and len(rose_symbols) >= 2:
            if not self._check_symbol_reachability(board, unassigned, rose_symbols):
                return False

        if rose_symbols and len(rose_symbols) == 1:
            comps = _get_all_components(unassigned, board, self._pre_boundaries)
            sym_type = rose_symbols[0]
            for comp in comps:
                sym_count = sum(1 for (r, c) in comp
                                if board.cell(r, c).symbol == sym_type)
                if sym_count == 0 or len(comp) < sym_count:
                    return False

        return True

    def _check_symbol_reachability(self, board: Board, unassigned: set[tuple[int, int]],
                                    rose_symbols: list[str]) -> bool:
        for i, src_type in enumerate(rose_symbols):
            src_cells = [(r, c) for r, c in unassigned
                         if board.cell(r, c).symbol == src_type]
            if not src_cells:
                continue
            all_target_types = set(rose_symbols) - {src_type}
            target_cells = {(r, c) for r, c in unassigned
                            if board.cell(r, c).symbol in all_target_types}
            if not target_cells:
                continue
            same_type_barriers = {(r, c) for r, c in unassigned
                                  if board.cell(r, c).symbol == src_type}
            for sr, sc in src_cells:
                barriers = same_type_barriers - {(sr, sc)}
                if not self._bfs_reachable((sr, sc), target_cells, unassigned,
                                           barriers, board.height, board.width,
                                           self._pre_boundaries):
                    return False
        return True

    @staticmethod
    def _bfs_reachable(start: tuple[int, int], targets: set[tuple[int, int]],
                       unassigned: set[tuple[int, int]],
                       barriers: set[tuple[int, int]],
                       h: int, w: int,
                       pre_boundaries: set[tuple[int, int, int, int]] | None = None) -> bool:
        from collections import deque
        visited: set[tuple[int, int]] = set()
        q: deque[tuple[int, int]] = deque([start])
        while q:
            r, c = q.popleft()
            if (r, c) in visited:
                continue
            visited.add((r, c))
            if (r, c) in targets:
                return True
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if 0 <= nr < h and 0 <= nc < w:
                    if (nr, nc) in unassigned and (nr, nc) not in barriers:
                        if pre_boundaries:
                            key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
                            if key in pre_boundaries:
                                continue
                        if (nr, nc) not in visited:
                            q.append((nr, nc))
        return False

    def _pick_seed(self, unassigned: set[tuple[int, int]]) -> tuple[int, int]:
        """Multi-priority seed selection (inspired by Neptune17 AoG_Solver).

        Priority cascade: puzzle_piece → area clues → rose symbols → compass →
        constraint-adjacent → corners → isolated → top-left.
        """
        board = getattr(self, '_board', None)

        if board is not None and self.puzzle.has_rule("puzzle_piece"):
            pieces = [p for p in unassigned if board.cell(p[0], p[1]).shape_pattern is not None]
            if pieces:
                return min(pieces)

        if board is not None and self.puzzle.has_rule("area"):
            clue_seeds = [(p, board.cell(p[0], p[1]).number) for p in unassigned
                          if board.cell(p[0], p[1]).number is not None]
            if clue_seeds:
                clue_seeds.sort(key=lambda x: x[1])
                return clue_seeds[0][0]

        if board is not None and self.puzzle.has_rule("rose_window"):
            rose_s = _rose_symbol_types(self.puzzle, board)
            if rose_s:
                sym_seeds = [p for p in unassigned if board.cell(p[0], p[1]).symbol is not None]
                if sym_seeds:
                    return min(sym_seeds)

        if board is not None and self.puzzle.has_rule("compass"):
            compass_s = [p for p in unassigned if board.cell(p[0], p[1]).compass is not None]
            if compass_s:
                return min(compass_s)

        if board is not None:
            constraint_seed = self._find_constraint_adjacent_seed(unassigned)
            if constraint_seed:
                return constraint_seed

            corner = self._find_corner_seed(unassigned)
            if corner:
                return corner

            isolated = self._find_isolated_seed(unassigned)
            if isolated:
                return isolated

        return min(unassigned)

    def _count_blocked_sides(self, r: int, c: int) -> int:
        cnt = 0
        for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
            nr, nc = r + dr, c + dc
            if not (0 <= nr < self._board.height and 0 <= nc < self._board.width):
                cnt += 1
                continue
            nb = self._board.cell(nr, nc)
            if nb.blocked or nb.assigned:
                cnt += 1
                continue
            key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
            if key in self._pre_boundaries:
                cnt += 1
        return cnt

    def _find_isolated_seed(self, unassigned: set[tuple[int, int]]) -> tuple[int, int] | None:
        for p in sorted(unassigned):
            if self._count_blocked_sides(p[0], p[1]) == 4:
                return p
        return None

    def _find_corner_seed(self, unassigned: set[tuple[int, int]]) -> tuple[int, int] | None:
        for p in sorted(unassigned):
            if self._count_blocked_sides(p[0], p[1]) >= 3:
                return p
        return None

    def _find_constraint_adjacent_seed(self, unassigned: set[tuple[int, int]]) -> tuple[int, int] | None:
        assigned: set[tuple[int, int]] = set()
        for r in range(self._board.height):
            for c in range(self._board.width):
                if self._board.cell(r, c).assigned:
                    assigned.add((r, c))
        if not assigned:
            return None
        for r, c in sorted(unassigned):
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if (nr, nc) in assigned:
                    key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
                    if key in self._pre_boundaries:
                        continue
                    edge = self._board.edge_between(r, c, nr, nc)
                    if edge is not None and edge.constraint is not None:
                        return (r, c)
        return None

    def _solve_rose_growth(self, board: Board,
                           all_positions: set[tuple[int, int]]) -> dict[int, set[tuple[int, int]]] | None:
        from src.solver.region_match import solve_by_region_match
        result = solve_by_region_match(self.puzzle, board, self._pre_boundaries)
        if result is not None:
            return result

        from src.solver.rose_growth import solve_rose_growth
        return solve_rose_growth(self.puzzle, board, self._pre_boundaries)

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

    def _try_exact_cover(self, all_positions: set[tuple[int, int]]) -> dict[int, set[tuple[int, int]]] | None:
        """Pre-pass: exact-cover for bounded-candidate puzzles (≈ glimmith-solver / aog DLX).

        Only applicable when region candidates are bounded (shape_pool, block, precise).
        Pre-generates ALL legal candidates, then solves as exact cover with MRV heuristic.
        """
        puzzle = self.puzzle
        board = self._board

        if not puzzle.has_rule("shape_pool") and not puzzle.has_rule("block") and not puzzle.has_rule("precise"):
            return None
        if puzzle.has_rule("shape_pool"):
            pool = puzzle.get_rule("shape_pool")
            if pool is not None:
                shapes = pool.params.get("shapes", [])
                if not shapes:
                    return None
                total = sum(1 for _ in all_positions)
                max_placements = sum(len(_all_transformations(s.cells)) for s in shapes)
                if total * max_placements > 100000:
                    return None
        elif puzzle.has_rule("precise") and not puzzle.has_rule("block"):
            if puzzle.height * puzzle.width > 25:
                return None

        candidates = self._generate_all_candidates(all_positions)
        if not candidates:
            return None

        from collections import defaultdict
        cell_to_cands: dict[tuple[int, int], list[int]] = defaultdict(list)
        for idx, cc in enumerate(candidates):
            for c in cc:
                cell_to_cands[c].append(idx)
        for pos in sorted(all_positions):
            if pos not in cell_to_cands or not cell_to_cands[pos]:
                # Reset board for clean fallback
                for r in range(board.height):
                    for c in range(board.width):
                        board.cell(r, c).region_id = None
                return None

        conflict_matrix = self._build_ec_conflicts(candidates, all_positions)

        selected: list[int] = []
        covered: set[tuple[int, int]] = set()
        result: dict[int, set[tuple[int, int]]] | None = None

        def dfs() -> bool:
            nonlocal result
            self.steps += 1
            elapsed = time.monotonic() - self.start_time
            if elapsed > self.timeout or self.steps % 2000 == 0 and time.monotonic() - self.start_time > self.timeout:
                return False

            if len(covered) == len(all_positions):
                regions: dict[int, set[tuple[int, int]]] = {}
                for ri, ci in enumerate(selected):
                    regions[ri] = set(candidates[ci])
                    for r, c in regions[ri]:
                        board.cell(r, c).region_id = ri
                if self._check_global_constraints(board, regions):
                    result = regions
                    return True
                return False

            best_cell, best_opts = None, None
            for cell in sorted(all_positions):
                if cell in covered:
                    continue
                opts = []
                for ci in cell_to_cands[cell]:
                    cc = candidates[ci]
                    if any(c in covered for c in cc):
                        continue
                    if any(si in conflict_matrix[ci] for si in selected):
                        continue
                    opts.append(ci)
                if not opts:
                    return False
                if best_opts is None or len(opts) < len(best_opts):
                    best_cell = cell
                    best_opts = opts
                    if len(best_opts) == 1:
                        break
            if best_opts is None:
                return False

            for ci in best_opts:
                selected.append(ci)
                for c in candidates[ci]:
                    covered.add(c)
                if dfs():
                    return True
                selected.pop()
                for c in candidates[ci]:
                    covered.discard(c)
            return False

        if dfs() and result is not None:
            return result
        # Reset region IDs on failure
        for r in range(board.height):
            for c in range(board.width):
                board.cell(r, c).region_id = None
        return None

    def _generate_all_candidates(self, all_positions: set[tuple[int, int]]) -> list[set[tuple[int, int]]]:
        puzzle = self.puzzle
        board = self._board
        results: list[set[tuple[int, int]]] = []

        if puzzle.has_rule("shape_pool"):
            pool_rule = puzzle.get_rule("shape_pool")
            if pool_rule is not None:
                pool_shapes = pool_rule.params.get("shapes", [])
                seen: set[frozenset[tuple[int, int]]] = set()
                for seed in sorted(all_positions):
                    sr, sc = seed
                    for ps in pool_shapes:
                        for tf in _all_transformations(ps.cells):
                            for rs, cs in tf:
                                dr, dc = sr - rs, sc - cs
                                placed: set[tuple[int, int]] = set()
                                valid = True
                                for r2, c2 in tf:
                                    nr, nc = r2 + dr, c2 + dc
                                    if (nr, nc) not in all_positions:
                                        valid = False
                                        break
                                    if board.cell(nr, nc).blocked:
                                        valid = False
                                        break
                                    placed.add((nr, nc))
                                if not valid:
                                    continue
                                placed_fs = frozenset(placed)
                                if placed_fs in seen:
                                    continue
                                seen.add(placed_fs)
                                if not self._region_feasible(board, placed):
                                    continue
                                results.append(placed)
                return results

        if puzzle.has_rule("block"):
            prec_area = puzzle.get_rule("precise").params["area"] if puzzle.has_rule("precise") else None
            seen: set[frozenset[tuple[int, int]]] = set()
            for h in range(1, puzzle.height + 1):
                for w in range(1, puzzle.width + 1):
                    area = h * w
                    if prec_area is not None and area != prec_area:
                        continue
                    for r in range(puzzle.height - h + 1):
                        for c in range(puzzle.width - w + 1):
                            cells = frozenset((r + dr, c + dc) for dr in range(h) for dc in range(w))
                            if cells in seen:
                                continue
                            seen.add(cells)
                            cell_set = set(cells)
                            if not cell_set.issubset(all_positions):
                                continue
                            if not self._region_feasible(board, cell_set):
                                continue
                            results.append(cell_set)
            return results

        if puzzle.has_rule("precise"):
            target = puzzle.get_rule("precise").params["area"]
            seen: set[frozenset[tuple[int, int]]] = set()
            for seed in sorted(all_positions):
                for cand in _bfs_fixed_shape(board, seed, all_positions, target,
                                             self._pre_boundaries, self):
                    fs = frozenset(cand)
                    if fs not in seen:
                        seen.add(fs)
                        if self._region_feasible(board, cand):
                            results.append(cand)
                        if len(results) >= 8000:
                            return results
            return results

        return results

    def _build_ec_conflicts(self, candidates: list[set[tuple[int, int]]],
                            all_positions: set[tuple[int, int]]) -> list[set[int]]:
        n = len(candidates)
        conflicts: list[set[int]] = [set() for _ in range(n)]
        puzzle = self.puzzle
        board = self._board
        cell_to_cand: dict[tuple[int, int], int] = {}
        for idx, cc in enumerate(candidates):
            for c in cc:
                cell_to_cand[c] = idx

        for pos in sorted(all_positions):
            if pos not in cell_to_cand:
                continue
            ci = cell_to_cand[pos]
            r, c = pos
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if 0 <= nr < board.height and 0 <= nc < board.width:
                    if (nr, nc) in cell_to_cand:
                        cj = cell_to_cand[(nr, nc)]
                        if ci == cj:
                            continue
                        ci_cells, cj_cells = candidates[ci], candidates[cj]

                        if puzzle.has_rule("heterogeneous") or puzzle.has_rule("homogeneous"):
                            edge = board.edge_between(r, c, nr, nc)
                            if edge is not None and edge.constraint is not None:
                                from src.models.board import EdgeConstraintType
                                s1 = Shape(cells=frozenset(ci_cells))
                                s2 = Shape(cells=frozenset(cj_cells))
                                eq = canonical_key(s1.cells) == canonical_key(s2.cells)
                                if edge.constraint.type == EdgeConstraintType.HETEROGENEOUS and eq:
                                    conflicts[ci].add(cj); conflicts[cj].add(ci)
                                if edge.constraint.type == EdgeConstraintType.HOMOGENEOUS and not eq:
                                    conflicts[ci].add(cj); conflicts[cj].add(ci)

                        if puzzle.has_rule("differentiation"):
                            if len(ci_cells) == len(cj_cells):
                                conflicts[ci].add(cj); conflicts[cj].add(ci)

                        if puzzle.has_rule("mixed"):
                            si = canonical_key(Shape(cells=frozenset(ci_cells)).cells)
                            sj = canonical_key(Shape(cells=frozenset(cj_cells)).cells)
                            if si == sj:
                                conflicts[ci].add(cj); conflicts[cj].add(ci)

        def _shape_key(idx: int) -> str:
            return canonical_key(Shape(cells=frozenset(candidates[idx])).cells)

        if puzzle.has_rule("different"):
            for i in range(n):
                ki = _shape_key(i)
                for j in range(i + 1, n):
                    if _shape_key(j) == ki:
                        conflicts[i].add(j); conflicts[j].add(i)

        if puzzle.has_rule("same"):
            for i in range(n):
                ki = _shape_key(i)
                for j in range(i + 1, n):
                    if _shape_key(j) != ki:
                        conflicts[i].add(j); conflicts[j].add(i)

        return conflicts


def _all_transformations(cells: frozenset[tuple[int, int]]) -> list[frozenset[tuple[int, int]]]:
    from src.solver.shapes import rotate_90, rotate_180, rotate_270, flip_horizontal, flip_vertical
    s = cells
    r1 = rotate_90(s)
    r2 = rotate_180(s)
    r3 = rotate_270(s)
    fh = flip_horizontal(s)
    fv = flip_vertical(s)
    fh_r1 = rotate_90(fh)
    fh_r2 = rotate_180(fh)
    fh_r3 = rotate_270(fh)
    seen: set[frozenset[tuple[int, int]]] = set()
    result: list[frozenset[tuple[int, int]]] = []
    for t in [s, r1, r2, r3, fh, fv, fh_r1, fh_r2, fh_r3]:
        if t not in seen:
            seen.add(t)
            result.append(t)
    return result


def _bfs_fixed_shape(board, seed, unassigned, target_size, pre_boundaries, solver):
    from collections import deque
    results: list[set[tuple[int, int]]] = []
    seen: set[frozenset[tuple[int, int]]] = set()
    budget = 0

    def _enumerate(current: set[tuple[int, int]], frontier: set[tuple[int, int]]):
        nonlocal budget
        if len(current) == target_size:
            fs = frozenset(current)
            if fs not in seen:
                seen.add(fs)
                results.append(set(current))
            return
        if len(results) >= 5000:
            return
        for cell in sorted(frontier):
            new_region = current | {cell}
            new_frontier = (frontier - {cell}) | {
                nb for r, c in [cell] for nb in [(r - 1, c), (r + 1, c), (r, c - 1), (r, c + 1)]
                if nb in unassigned and nb not in new_region
            }
            budget += 1
            if budget % 5000 == 0:
                import time as _t
                if _t.monotonic() - solver.start_time > solver.timeout:
                    return
            if not solver._region_feasible(board, new_region):
                continue
            _enumerate(new_region, new_frontier)

    initial = {seed}
    frontier = set()
    for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
        nr, nc = seed[0] + dr, seed[1] + dc
        if (nr, nc) in unassigned:
            frontier.add((nr, nc))
    _enumerate(initial, frontier)
    return results


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
    _rose_stop_expanding as _rose_stop_expanding_fn,
    _has_size_constraint as _has_size_constraint_fn,
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
BacktrackSolver._rose_stop_expanding = _rose_stop_expanding_fn
BacktrackSolver._has_size_constraint = _has_size_constraint_fn
BacktrackSolver._check_compass_final = _check_compass_final_fn
