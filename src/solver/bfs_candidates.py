"""Complete BFS-based candidate generation for region matching."""
from __future__ import annotations

from collections import deque


def generate_all_candidates(
    board,
    seed: tuple[int, int],
    all_positions: set[tuple[int, int]],
    pre_boundaries: set[tuple[int, int, int, int]],
    cell_symbol: dict[tuple[int, int], str],
    symbol_types: list[str] | None = None,
    max_cells: int = 100,
) -> list[set[tuple[int, int]]]:
    """Generate ALL boundary-compliant connected subsets containing the seed.

    Uses BFS over subsets, ensuring no subset is missed due to DFS ordering.
    Deduplication is built-in (visited set of frozensets).

    For multi-symbol: only candidates containing ALL symbol_types are returned.
    For 1-symbol: all connected subsets are returned (each seed IS a region).
    """
    h, w = board.height, board.width
    sr, sc = seed
    seed_sym = cell_symbol.get(seed)

    if symbol_types is None:
        symbol_types = []
    all_required = set(symbol_types)
    is_multi = len(all_required) >= 2

    # Track visited subsets to avoid duplicates
    visited: set[frozenset] = set()
    results: list[set[tuple[int, int]]] = []

    # Queue of (current_subset_as_frozenset, frontier_cells, symbols_in_region)
    initial = frozenset({seed})
    visited.add(initial)
    initial_syms = frozenset({seed_sym}) if seed_sym else frozenset()

    # Compute initial frontier
    initial_frontier = frozenset(
        (nr, nc) for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]
        if 0 <= (nr := sr + dr) < h and 0 <= (nc := sc + dc) < w
        and (nr, nc) in all_positions
        and _edge_key(sr, sc, nr, nc) not in pre_boundaries
    )

    queue: deque[tuple[frozenset, frozenset, frozenset]] = deque()
    queue.append((initial, initial_frontier, initial_syms))

    while queue and len(results) < 20000:
        current_fs, frontier_fs, syms_fs = queue.popleft()
        current = set(current_fs)
        current_syms = set(syms_fs)

        # Add to results if complete (has all required symbols)
        if is_multi:
            if current_syms == all_required:
                results.append(current)
        else:
            results.append(current)

        # Don't expand beyond max_cells
        if len(current) >= max_cells:
            continue

        # Try adding each frontier cell
        for cell in frontier_fs:
            cr, cc = cell
            cell_sym = cell_symbol.get(cell)

            skip = False

            # Check symbol constraint: if multi-symbol, can't add duplicate symbol type
            if cell_sym is not None and is_multi:
                if cell_sym in current_syms:
                    skip = True

            # Check boundaries: would adding (cr,cc) create two current cells
            # that are adjacent and separated by a boundary?
            if not skip:
                for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                    nr, nc = cr + dr, cc + dc
                    if (nr, nc) in current:
                        key = _edge_key(cr, cc, nr, nc)
                        if key in pre_boundaries:
                            skip = True
                            break

            if skip:
                continue

            # Create new subset
            new_fs = frozenset(current | {cell})
            if new_fs in visited:
                continue
            visited.add(new_fs)

            # Update symbols
            new_syms = current_syms | ({cell_sym} if cell_sym else set())
            new_syms_fs = frozenset(new_syms)

            # Compute new frontier: old frontier minus this cell,
            # plus new neighbors of this cell
            new_frontier_extra = []
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = cr + dr, cc + dc
                if (0 <= nr < h and 0 <= nc < w
                        and (nr, nc) in all_positions
                        and (nr, nc) not in current
                        and _edge_key(cr, cc, nr, nc) not in pre_boundaries):
                    new_frontier_extra.append((nr, nc))

            new_frontier_fs = frozenset(
                set(c for c in frontier_fs if c != cell) | set(new_frontier_extra)
            )

            queue.append((new_fs, new_frontier_fs, new_syms_fs))

    return results


def _edge_key(r1: int, c1: int, r2: int, c2: int) -> tuple[int, int, int, int]:
    if r1 < r2 or (r1 == r2 and c1 < c2):
        return (r1, c1, r2, c2)
    return (r2, c2, r1, c1)
