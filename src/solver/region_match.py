"""Region matching solver for rose_window puzzles.

Algorithm:
  1. Pick the most constrained symbol type (fewest seeds)
  2. For each seed of that type, find ALL legal complete regions (concurrently)
  3. Match: find one region per seed that together cover all fillable cells
     without overlap, satisfying all constraints.
"""
from __future__ import annotations

from collections import deque

from src.models.board import Board
from src.models.puzzle import Puzzle
from src.solver.constraints import _rose_symbol_types, _rose_M, check_boundary_consistency


def _check_boundaries_partial(
    board: Board, pre_boundaries: set[tuple[int, int, int, int]]
) -> bool:
    """Check boundary constraints for partial assignment."""
    for r1, c1, r2, c2 in pre_boundaries:
        rid1 = board.cell(r1, c1).region_id
        rid2 = board.cell(r2, c2).region_id
        if rid1 is not None and rid2 is not None and rid1 == rid2:
            return False
    return True


def _can_partition(
    remaining: set[tuple[int, int]],
    seed_cells: set[tuple[int, int]],
    pre_boundaries: set[tuple[int, int, int, int]],
    board: Board,
    min_component_cells: int = 1,
) -> bool:
    """Check if `remaining` can potentially be partitioned into valid regions.
    
    1. Every cell in `remaining` must be reachable from at least one seed.
    2. Each connected component must have at least `min_component_cells` cells
       (e.g., for multi-symbol, a region needs at least N cells for N symbol types).
    """
    h, w = board.height, board.width
    active_seeds = seed_cells & remaining
    if not active_seeds:
        return len(remaining) == 0

    # Single BFS from all seeds to check reachability AND find components
    visited: set[tuple[int, int]] = set()
    queue: deque[tuple[int, int]] = deque(active_seeds)

    while queue:
        r, c = queue.popleft()
        if (r, c) in visited:
            continue
        visited.add((r, c))
        for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
            nr, nc = r + dr, c + dc
            if 0 <= nr < h and 0 <= nc < w:
                if (nr, nc) in remaining and (nr, nc) not in visited:
                    key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
                    if key not in pre_boundaries:
                        queue.append((nr, nc))

    # Check 1: all remaining cells reachable from some seed
    if visited != remaining:
        return False

    # Check 2: each connected component has enough cells
    if min_component_cells <= 1:
        return True

    # Find components among remaining cells
    remaining_list = list(remaining)
    comp_visited: set[tuple[int, int]] = set()
    for cell in remaining_list:
        if cell in comp_visited:
            continue
        comp: set[tuple[int, int]] = set()
        q = deque([cell])
        while q:
            r, c = q.popleft()
            if (r, c) in comp_visited:
                continue
            comp_visited.add((r, c))
            comp.add((r, c))
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if 0 <= nr < h and 0 <= nc < w and (nr, nc) in remaining:
                    if (nr, nc) not in comp_visited:
                        key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
                        if key not in pre_boundaries:
                            q.append((nr, nc))
        if len(comp) < min_component_cells:
            return False

    return True


def solve_by_region_match(
    puzzle: Puzzle,
    board: Board,
    pre_boundaries: set[tuple[int, int, int, int]],
) -> dict[int, set[tuple[int, int]]] | None:
    """Solve rose_window by precomputing all legal regions then matching."""
    symbol_types = _rose_symbol_types(puzzle, board)
    if not symbol_types:
        return None
    M = _rose_M(puzzle, board)
    if M <= 0:
        return None

    all_positions = {(r, c) for r in range(board.height) for c in range(board.width)
                     if not board.cell(r, c).blocked}
    total_cells = len(all_positions)

    # Step 1: pick the most constrained symbol type
    best_type = symbol_types[0]
    best_count = 999
    for st in symbol_types:
        count = sum(1 for r in range(board.height) for c in range(board.width)
                    if board.cell(r, c).symbol == st and not board.cell(r, c).blocked)
        if count < best_count:
            best_count = count
            best_type = st

    seeds = sorted((r, c) for r in range(board.height) for c in range(board.width)
                   if board.cell(r, c).symbol == best_type
                   and not board.cell(r, c).blocked)
    if len(seeds) != M:
        return None

    # Collect all symbol cells (for reachability checks)
    all_seed_cells = {(r, c) for r in range(board.height) for c in range(board.width)
                      if board.cell(r, c).symbol is not None and not board.cell(r, c).blocked}

    # Step 2: generate all legal complete regions for each seed
    from src.solver.backtrack import BacktrackSolver
    solver = BacktrackSolver(puzzle)

    # Build cell -> symbol mapping
    cell_symbol: dict[tuple[int, int], str] = {}
    for r in range(board.height):
        for c in range(board.width):
            sym = board.cell(r, c).symbol
            if sym:
                cell_symbol[(r, c)] = sym

    all_candidates: list[list[set[tuple[int, int]]]] = []
    if len(symbol_types) == 1:
        from src.solver.bfs_candidates import generate_all_candidates
        for seed in seeds:
            cands = generate_all_candidates(
                board, seed, all_positions, pre_boundaries,
                cell_symbol, symbol_types,
            )
            if not cands:
                return None
            all_candidates.append(cands)
    else:
        for seed in seeds:
            gen_board = solver._board_from_puzzle()
            candidates = solver._generate_region_candidates(gen_board, seed, all_positions)
            candidates = [c for c in candidates
                          if {board.cell(r, c).symbol for r, c in c
                              if board.cell(r, c).symbol is not None} == set(symbol_types)]
            valid = []
            for cand in candidates:
                ok = True
                for (r1, c1) in cand:
                    for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                        r2, c2 = r1 + dr, c1 + dc
                        if (r2, c2) in cand:
                            key = (r1, c1, r2, c2) if r1 < r2 or (r1 == r2 and c1 < c2) else (r2, c2, r1, c1)
                            if key in pre_boundaries:
                                ok = False
                                break
                    if not ok:
                        break
                if ok:
                    valid.append(cand)
            if not valid:
                return None
            all_candidates.append(valid)

    # Step 3: pre-filter candidates by area constraints
    # Each candidate must be large enough to hold its seed, but small enough
    # to leave room for remaining seeds
    for i in range(len(all_candidates)):
        filtered = []
        for cand in all_candidates[i]:
            sz = len(cand)
            # Minimum: must contain the seed (at least 1 cell)
            # Maximum: must leave at least 1 cell per remaining seed
            max_for_this = total_cells - (M - 1)  # others need min 1 cell each
            if sz <= max_for_this:
                filtered.append(cand)
        if not filtered:
            return None
        all_candidates[i] = filtered

    # Sort by size ascending (small first = better for matching when space is tight)
    # Keep top candidates for speed
    # Step 3: pre-filter candidates (for BFS-generated 1-symbol candidates)
    if len(symbol_types) == 1:
        N = len(symbol_types)
        for i in range(len(all_candidates)):
            filtered = []
            for cand in all_candidates[i]:
                remaining = all_positions - cand
                if _can_partition(remaining, all_seed_cells, pre_boundaries, board, N):
                    filtered.append(cand)
            if not filtered:
                return None
            all_candidates[i] = filtered

    for i in range(len(all_candidates)):
        all_candidates[i].sort(key=len)

    for i in range(len(all_candidates)):
        all_candidates[i].sort(key=len)

    # Step 4: match
    val_board = solver._board_from_puzzle()

    import time as _time
    t_start = _time.monotonic()
    result = _match_regions(
        val_board, pre_boundaries, symbol_types,
        all_candidates, all_positions, all_seed_cells,
        total_cells, 0, set(), {},
        t_start, 10.0,
    )
    if result is not None:
        from src.solver.validator import SolutionValidator
        for rid, cells in result.items():
            for r, c in cells:
                val_board.cell(r, c).region_id = rid
        v = SolutionValidator()
        val = v.validate(puzzle, val_board)
        if val.solved:
            for r in range(board.height):
                for c in range(board.width):
                    board.cell(r, c).region_id = val_board.cell(r, c).region_id
            return result

    return None


def _match_regions(
    board: Board,
    pre_boundaries: set[tuple[int, int, int, int]],
    symbol_types: list[str],
    all_candidates: list[list[set[tuple[int, int]]]],
    all_positions: set[tuple[int, int]],
    all_seed_cells: set[tuple[int, int]],
    total_cells: int,
    seed_idx: int,
    covered: set[tuple[int, int]],
    assignment: dict[int, set[tuple[int, int]]],
    _start_time: float = 0.0,
    _timeout: float = 10.0,
) -> dict[int, set[tuple[int, int]]] | None:
    """Recursively match one region per seed, ensuring no overlap."""
    import time
    if _start_time > 0 and time.monotonic() - _start_time > _timeout:
        return None

    if seed_idx == len(all_candidates):
        if covered == all_positions:
            for rid, cells in assignment.items():
                for r, c in cells:
                    board.cell(r, c).region_id = rid
            if check_boundary_consistency(board):
                return assignment
            for rid in assignment:
                for r, c in assignment[rid]:
                    board.cell(r, c).region_id = None
        return None

    remaining_seeds = len(all_candidates) - seed_idx - 1
    remaining_cells = total_cells - len(covered)

    for cand in all_candidates[seed_idx]:
        sz = len(cand)
        
        # Opt 1: area check — this candidate must leave enough cells for remaining seeds
        if sz > remaining_cells - remaining_seeds:
            continue  # remaining seeds need at least 1 cell each
        
        # Opt 2: no overlap
        if cand & covered:
            continue

        # Opt 3: early boundary check
        for r, c in cand:
            board.cell(r, c).region_id = seed_idx
        boundary_ok = _check_boundaries_partial(board, pre_boundaries)
        if not boundary_ok:
            for r, c in cand:
                board.cell(r, c).region_id = None
            continue

        new_covered = covered | cand

        # Forward check: remaining seeds must each have a candidate subset of remaining
        new_remaining = all_positions - new_covered

        # Opt 5: forward check — remaining seeds must each have a candidate
        feasible = True
        for si in range(seed_idx + 1, len(all_candidates)):
            if not any(c.issubset(new_remaining) for c in all_candidates[si]):
                feasible = False
                break
        if not feasible:
            for r, c in cand:
                board.cell(r, c).region_id = None
            continue

        result = _match_regions(
            board, pre_boundaries, symbol_types,
            all_candidates, all_positions, all_seed_cells,
            total_cells,
            seed_idx + 1, new_covered, {**assignment, seed_idx: cand},
            _start_time, _timeout,
        )
        if result is not None:
            return result

        for r, c in cand:
            board.cell(r, c).region_id = None

    return None
