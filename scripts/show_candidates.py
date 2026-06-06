"""Show all valid region candidates for a rose_window puzzle visually.

Uses the BFS-based exhaustive candidate generation.
"""
from __future__ import annotations

import json
import sys
from collections import Counter

sys.path.insert(0, '.')

from src.io.puzzle_codec import dict_to_puzzle
from src.solver.backtrack import BacktrackSolver
from src.solver.constraints import _rose_symbol_types, _rose_M
from src.solver.bfs_candidates import generate_all_candidates
from src.solver.region_match import _can_partition


def show(puzzle_path: str, max_show: int = 200) -> None:
    """Show candidates for a puzzle.

    Args:
        puzzle_path: Path to puzzle JSON
        max_show: Max candidates to display per seed (default 200, set 0 for all)
    """
    with open(puzzle_path, encoding='utf-8') as f:
        puzzle = dict_to_puzzle(json.load(f))

    solver = BacktrackSolver(puzzle)
    board = solver._board_from_puzzle()
    pre = solver._pre_boundaries
    h, w = puzzle.height, puzzle.width
    all_pos = {(r, c) for r in range(h) for c in range(w)
               if not board.cell(r, c).blocked}
    sym_types = _rose_symbol_types(puzzle, board)
    M = _rose_M(puzzle, board)
    total_cells = len(all_pos)

    # Build cell->symbol mapping
    cell_symbol: dict[tuple[int, int], str] = {}
    all_seed_cells: set[tuple[int, int]] = set()
    for r in range(h):
        for c in range(w):
            sym = board.cell(r, c).symbol
            if sym:
                cell_symbol[(r, c)] = sym
                all_seed_cells.add((r, c))

    # Header
    print(f'=== {puzzle_path} ===')
    print(f'Grid: {h}x{w}  Fillable: {total_cells}  Regions: {M}')
    print(f'Symbol types: {sym_types}')
    for st in sym_types:
        cells = [(r, c) for (r, c) in all_seed_cells if board.cell(r, c).symbol == st]
        print(f'  {st}: {cells}')
    print(f'Walls ({len(pre)}):')
    for r1, c1, r2, c2 in sorted(pre):
        d = 'v' if c1 == c2 else 'h'
        print(f'  {d} ({r1},{c1})-({r2},{c2})')

    # Grid with boundaries
    print('\nGrid:')
    h_edges: set[tuple[int, int]] = set()
    v_edges: set[tuple[int, int]] = set()
    for r1, c1, r2, c2 in pre:
        if r1 == r2:
            h_edges.add((r1, min(c1, c2)))
        else:
            v_edges.add((min(r1, r2), c1))

    header = '   '
    for c in range(w):
        header += f' {c}  '
    print(header)
    print('  +' + '---+' * w)
    for r in range(h):
        row = f'{r} |'
        for c in range(w):
            s = board.cell(r, c).symbol or '·'
            b = 'X' if board.cell(r, c).blocked else ' '
            row += f'{b}{s} '
            if c < w - 1:
                sep = '|' if (r, c) in h_edges else ' '
                row += sep
        row += '|'
        print(row)
        if r < h - 1:
            sep_row = '  +'
            for c in range(w):
                sep = '---' if (r, c) in v_edges else '   '
                sep_row += sep + '+'
            print(sep_row)
    print('  +' + '---+' * w)

    # Find seeds of the most constrained type
    best_type = sym_types[0]
    best_count = 999
    for st in sym_types:
        count = sum(1 for (r, c) in all_seed_cells if board.cell(r, c).symbol == st)
        if count < best_count:
            best_count = count
            best_type = st

    seeds = sorted((r, c) for r in range(h) for c in range(w)
                   if board.cell(r, c).symbol == best_type
                   and not board.cell(r, c).blocked)
    print(f'\nSeeds (most constrained type: {best_type}): {seeds}')

    # Generate and show candidates for each seed
    for seed_idx, seed in enumerate(seeds):
        print(f'\n{"=" * 60}')
        print(f'Seed ({seed[0]},{seed[1]}) candidates:')
        print(f'{"=" * 60}')

        # Generate ALL candidates via BFS
        import time
        t0 = time.monotonic()
        cands = generate_all_candidates(
            board, seed, all_pos, pre,
            cell_symbol, sym_types,
        )
        t1 = time.monotonic()
        print(f'Generated {len(cands)} total in {(t1-t0)*1000:.0f}ms')

        # Connectivity pre-filter
        N = len(sym_types)
        passing = []
        for cand in cands:
            remaining = all_pos - cand
            if _can_partition(remaining, all_seed_cells, pre, board, N):
                passing.append(cand)
        print(f'Connectivity pre-filter: {len(passing)}/{len(cands)} pass (min {N} cells/component)')

        # Sort by size
        cands.sort(key=len)

        # Size stats
        if cands:
            sizes = [len(c) for c in cands]
            print(f'Size range: {min(sizes)}-{max(sizes)}  (avg={total_cells/M:.1f})')
            size_counts = Counter(sizes)
            if len(size_counts) <= 30:
                print(f'Size distribution: {dict(sorted(size_counts.items()))}')
            else:
                print(f'Unique sizes: {len(size_counts)}')
        else:
            print('NO VALID CANDIDATES!')
            continue

        # Show candidates
        show_count = min(len(cands), max_show) if max_show > 0 else len(cands)
        for idx in range(show_count):
            region = cands[idx]
            syms_in = {cell_symbol.get((r, c)) for r, c in region if (r, c) in cell_symbol}
            print(f'\n  #{idx}  size={len(region)}  symbols={syms_in}')
            for r in range(h):
                row_str = f'    {r} '
                for c in range(w):
                    if (r, c) in region:
                        row_str += '[#]'
                    elif board.cell(r, c).symbol:
                        row_str += f' {board.cell(r, c).symbol} '
                    elif board.cell(r, c).blocked:
                        row_str += ' X '
                    else:
                        row_str += ' · '
                print(row_str)

        if len(cands) > max_show > 0:
            print(f'\n  ... ({len(cands) - max_show} more not shown)')


if __name__ == '__main__':
    path = sys.argv[1] if len(sys.argv) > 1 else 'puzzles/official/C/C4-1.json'
    max_show = int(sys.argv[2]) if len(sys.argv) > 2 else 200
    show(path, max_show)
