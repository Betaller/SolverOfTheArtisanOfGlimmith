"""Rose window puzzle solver using BFS + constraint repair."""
from __future__ import annotations

from collections import deque

from src.models.board import Board
from src.models.puzzle import Puzzle
from src.solver.constraints import _rose_symbol_types, _rose_M, check_boundary_consistency


def solve_rose_growth(puzzle: Puzzle, board: Board,
                      pre_boundaries: set[tuple[int, int, int, int]]) -> dict[int, set[tuple[int, int]]] | None:
    """Solve a rose_window puzzle using BFS growth + constraint-aware repair."""
    symbol_types = _rose_symbol_types(puzzle, board)
    if not symbol_types:
        return None
    M = _rose_M(puzzle, board)
    if M <= 0:
        return None

    first_type = symbol_types[0]
    seeds = [(r, c) for r in range(board.height) for c in range(board.width)
             if board.cell(r, c).symbol == first_type and not board.cell(r, c).blocked]
    if len(seeds) != M:
        return None

    # Multi-symbol support: run BFS from first-type seeds, collect symbols
    if len(symbol_types) >= 2:
        return _solve_multisymbol(puzzle, board, pre_boundaries, symbol_types, seeds, M)

    # 1-symbol type: BFS + orphan brute-force
    return _solve_singlesymbol(puzzle, board, pre_boundaries, seeds, M)


def _solve_singlesymbol(
    puzzle: Puzzle, board: Board,
    pre_boundaries: set[tuple[int, int, int, int]],
    seeds: list[tuple[int, int]], M: int
) -> dict[int, set[tuple[int, int]]] | None:
    """Solve 1-symbol rose_window using wavefront growth + swap repair."""
    h, w = board.height, board.width
    all_positions = {(r, c) for r in range(h) for c in range(w)
                     if not board.cell(r, c).blocked}

    # Initialize: each seed is a region
    region_of: dict[tuple[int, int], int] = {}
    region_cells: list[set[tuple[int, int]]] = [set() for _ in range(M)]
    for i, (r, c) in enumerate(seeds):
        board.cell(r, c).region_id = i
        region_of[(r, c)] = i
        region_cells[i].add((r, c))

    unassigned = all_positions - set(seeds)

    # Wavefront assignment: assign cells adjacent to assigned cells first
    while unassigned:
        # Find the unassigned cell with the most assigned neighbors
        best_cell = None
        best_adj = []
        for r, c in unassigned:
            adj_regions: set[int] = set()
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if 0 <= nr < h and 0 <= nc < w:
                    nrid = board.cell(nr, nc).region_id
                    if nrid is not None:
                        key = _edge_key(r, c, nr, nc)
                        if key not in pre_boundaries:
                            adj_regions.add(nrid)
            if len(adj_regions) > len(best_adj):
                best_cell = (r, c)
                best_adj = list(adj_regions)
        if best_cell is None or not best_adj:
            break

        r, c = best_cell
        # Try each adjacent region, preferring the smallest one
        assigned = False
        for rid in sorted(best_adj, key=lambda i: len(region_cells[i])):
            if _would_violate(board, r, c, rid, pre_boundaries):
                continue
            board.cell(r, c).region_id = rid
            region_of[(r, c)] = rid
            region_cells[rid].add((r, c))
            unassigned.discard((r, c))
            assigned = True
            break
        if not assigned:
            # Force-assign to the best adjacent region
            rid = sorted(best_adj, key=lambda i: len(region_cells[i]))[0]
            board.cell(r, c).region_id = rid
            region_of[(r, c)] = rid
            region_cells[rid].add((r, c))
            unassigned.discard((r, c))

    # Swap repair: fix boundary violations by swapping cells between adjacent regions
    for _ in range(500):
        violations = []
        for r1, c1, r2, c2 in pre_boundaries:
            rid1 = board.cell(r1, c1).region_id
            rid2 = board.cell(r2, c2).region_id
            if rid1 is not None and rid2 is not None and rid1 == rid2:
                violations.append((r1, c1, r2, c2, rid1))

        if not violations:
            break

        fixed_any = False
        for r1, c1, r2, c2, rid_v in violations:
            # Try moving (r1,c1) or (r2,c2) to a different region
            for cell_r, cell_c in [(r1, c1), (r2, c2)]:
                cur_rid = board.cell(cell_r, cell_c).region_id
                if cur_rid is None:
                    continue
                alt_regions: set[int] = set()
                for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                    nr, nc = cell_r + dr, cell_c + dc
                    if 0 <= nr < h and 0 <= nc < w:
                        nrid = board.cell(nr, nc).region_id
                        if nrid is not None and nrid != cur_rid:
                            key = _edge_key(cell_r, cell_c, nr, nc)
                            if key not in pre_boundaries:
                                alt_regions.add(nrid)
                for nrid in sorted(alt_regions, key=lambda i: len(region_cells[i])):
                    conflict = False
                    for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                        nr, nc = cell_r + dr, cell_c + dc
                        if 0 <= nr < h and 0 <= nc < w:
                            if board.cell(nr, nc).region_id == nrid:
                                key = _edge_key(cell_r, cell_c, nr, nc)
                                if key in pre_boundaries:
                                    conflict = True
                                    break
                    if conflict:
                        continue
                    board.cell(cell_r, cell_c).region_id = nrid
                    region_cells[cur_rid].discard((cell_r, cell_c))
                    region_cells[nrid].add((cell_r, cell_c))
                    fixed_any = True
                    break
                if fixed_any:
                    break
            if fixed_any:
                break

        if fixed_any:
            continue  # Try another iteration

        # Chain move: move a neighbor first
        for r1, c1, r2, c2, rid_v in violations:
            for cell_r, cell_c in [(r1, c1), (r2, c2)]:
                cur_rid = board.cell(cell_r, cell_c).region_id
                if cur_rid is None:
                    continue
                for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                    nr, nc = cell_r + dr, cell_c + dc
                    if 0 <= nr < h and 0 <= nc < w:
                        n_rid = board.cell(nr, nc).region_id
                        if n_rid is None or n_rid == cur_rid:
                            continue
                        key = _edge_key(cell_r, cell_c, nr, nc)
                        if key in pre_boundaries:
                            continue
                        can_move_neighbor = True
                        for ddr, ddc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                            nnr, nnc = nr + ddr, nc + ddc
                            if 0 <= nnr < h and 0 <= nnc < w:
                                if (nnr, nnc) != (cell_r, cell_c):
                                    if board.cell(nnr, nnc).region_id == cur_rid:
                                        k2 = _edge_key(nr, nc, nnr, nnc)
                                        if k2 in pre_boundaries:
                                            can_move_neighbor = False
                                            break
                        if not can_move_neighbor:
                            continue
                        can_move_self = True
                        for ddr, ddc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                            nnr, nnc = cell_r + ddr, cell_c + ddc
                            if 0 <= nnr < h and 0 <= nnc < w:
                                if (nnr, nnc) != (nr, nc):
                                    if board.cell(nnr, nnc).region_id == n_rid:
                                        k2 = _edge_key(cell_r, cell_c, nnr, nnc)
                                        if k2 in pre_boundaries:
                                            can_move_self = False
                                            break
                        if can_move_self:
                            board.cell(nr, nc).region_id = cur_rid
                            region_cells[n_rid].discard((nr, nc))
                            region_cells[cur_rid].add((nr, nc))
                            board.cell(cell_r, cell_c).region_id = n_rid
                            region_cells[cur_rid].discard((cell_r, cell_c))
                            region_cells[n_rid].add((cell_r, cell_c))
                            fixed_any = True
                            break
                if fixed_any:
                    break
            if fixed_any:
                break

        if not fixed_any:
            break


def _solve_multisymbol(
    puzzle: Puzzle, board: Board,
    pre_boundaries: set[tuple[int, int, int, int]],
    symbol_types: list[str], seeds: list[tuple[int, int]], M: int
) -> dict[int, set[tuple[int, int]]] | None:
    """Solve multi-symbol rose_window using BFS + second pass + repair."""
    boundary_endpoints: set[tuple[int, int]] = set()
    for r1, c1, r2, c2 in pre_boundaries:
        boundary_endpoints.add((r1, c1))
        boundary_endpoints.add((r2, c2))

    region_symbols: list[set[str]] = [{symbol_types[0]} for _ in range(M)]
    region_sizes: list[int] = [1] * M

    for i, (r, c) in enumerate(seeds):
        board.cell(r, c).region_id = i

    queue: deque[tuple[int, int, int, int]] = deque()
    for i, (r, c) in enumerate(seeds):
        queue.append((r, c, i, 0))

    # BFS (same as original)
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
            key = _edge_key(r, c, nr, nc)
            if key in pre_boundaries:
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
                            k2 = _edge_key(nr, nc, nnr, nnc)
                            if k2 in pre_boundaries:
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

    # Second pass
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
                            key = _edge_key(r, c, nr, nc)
                            if key not in pre_boundaries:
                                candidates.add(nrid)
                if candidates:
                    sym = board.cell(r, c).symbol
                    valid = [i for i in candidates
                             if not (sym is not None and sym in region_symbols[i])]
                    if valid:
                        best = min(valid, key=lambda i: region_sizes[i])
                        board.cell(r, c).region_id = best
                        if sym is not None:
                            region_symbols[best].add(sym)
                        region_sizes[best] += 1
                        unassigned.discard((r, c))
                        changed = True

    # Repair
    for _ in range(200):
        repaired = False
        for r1, c1, r2, c2 in list(pre_boundaries):
            rid1 = board.cell(r1, c1).region_id
            rid2 = board.cell(r2, c2).region_id
            if rid1 is None or rid2 is None or rid1 != rid2:
                continue
            for (cell_r, cell_c), cur_rid in [((r1, c1), rid1), ((r2, c2), rid2)]:
                neigh_regions: set[int] = set()
                for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                    nr, nc = cell_r + dr, cell_c + dc
                    if 0 <= nr < board.height and 0 <= nc < board.width:
                        nrid = board.cell(nr, nc).region_id
                        if nrid is not None and nrid != cur_rid:
                            k2 = _edge_key(cell_r, cell_c, nr, nc)
                            if k2 not in pre_boundaries:
                                neigh_regions.add(nrid)
                sym = board.cell(cell_r, cell_c).symbol
                for nrid in sorted(neigh_regions):
                    if sym is not None and sym in region_symbols[nrid]:
                        continue
                    conflict = False
                    for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                        nr, nc = cell_r + dr, cell_c + dc
                        if 0 <= nr < board.height and 0 <= nc < board.width:
                            if board.cell(nr, nc).region_id == nrid:
                                k2 = _edge_key(cell_r, cell_c, nr, nc)
                                if k2 in pre_boundaries:
                                    conflict = True
                                    break
                    if conflict:
                        continue
                    region_symbols[cur_rid].discard(sym)
                    if sym is not None:
                        region_symbols[nrid].add(sym)
                    board.cell(cell_r, cell_c).region_id = nrid
                    region_sizes[cur_rid] -= 1
                    region_sizes[nrid] += 1
                    repaired = True
                    break
                if repaired:
                    break
            if repaired:
                break
        if not repaired:
            break

    if not all(len(syms) == len(symbol_types) for syms in region_symbols):
        return None
    if unassigned:
        return None

    return _build_regions(board)


def _would_violate(board: Board, r: int, c: int, rid: int,
                   pre_boundaries: set[tuple[int, int, int, int]]) -> bool:
    """Check if assigning (r,c) to rid would create a boundary violation."""
    for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
        nr, nc = r + dr, c + dc
        if 0 <= nr < board.height and 0 <= nc < board.width:
            if board.cell(nr, nc).region_id == rid:
                key = _edge_key(r, c, nr, nc)
                if key in pre_boundaries:
                    return True
    return False


def _edge_key(r1: int, c1: int, r2: int, c2: int) -> tuple[int, int, int, int]:
    if r1 < r2 or (r1 == r2 and c1 < c2):
        return (r1, c1, r2, c2)
    return (r2, c2, r1, c1)


def _build_regions(board: Board) -> dict[int, set[tuple[int, int]]]:
    regions: dict[int, set[tuple[int, int]]] = {}
    for r in range(board.height):
        for c in range(board.width):
            rid = board.cell(r, c).region_id
            if rid is not None:
                regions.setdefault(rid, set()).add((r, c))
    return regions
