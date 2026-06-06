from __future__ import annotations

from collections import deque

from src.models.board import Board, Shape
from src.solver.shapes import canonical_key, match_shape_pool


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


def _frontier(self, region: set[tuple[int, int]], unassigned: set[tuple[int, int]]) -> set[tuple[int, int]]:
    result: set[tuple[int, int]] = set()
    for r, c in region:
        for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
            nr, nc = r + dr, c + dc
            if (nr, nc) in unassigned and (nr, nc) not in region:
                result.add((nr, nc))
    return result


def _get_component(self, board: Board, seed: tuple[int, int],
                   unassigned: set[tuple[int, int]]) -> set[tuple[int, int]]:
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


def _count_components(unassigned: set[tuple[int, int]], board: Board,
                      pre_boundaries: set[tuple[int, int, int, int]]) -> int:
    if not pre_boundaries:
        return 1 if unassigned else 0
    visited: set[tuple[int, int]] = set()
    components = 0
    for seed in sorted(unassigned):
        if seed in visited:
            continue
        components += 1
        stack: list[tuple[int, int]] = [seed]
        while stack:
            r, c = stack.pop()
            if (r, c) in visited:
                continue
            visited.add((r, c))
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if (nr, nc) in unassigned and (nr, nc) not in visited:
                    key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
                    if key not in pre_boundaries:
                        stack.append((nr, nc))
    return components


def _get_all_components(unassigned: set[tuple[int, int]], board: Board,
                        pre_boundaries: set[tuple[int, int, int, int]]) -> list[set[tuple[int, int]]]:
    """Return each connected component (ignoring pre-boundary edges) as a separate set."""
    if not pre_boundaries:
        return [set(unassigned)] if unassigned else []
    visited: set[tuple[int, int]] = set()
    result: list[set[tuple[int, int]]] = []
    for seed in sorted(unassigned):
        if seed in visited:
            continue
        comp: set[tuple[int, int]] = set()
        stack: list[tuple[int, int]] = [seed]
        while stack:
            r, c = stack.pop()
            if (r, c) in visited:
                continue
            visited.add((r, c))
            comp.add((r, c))
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if (nr, nc) in unassigned and (nr, nc) not in visited:
                    key = (r, c, nr, nc) if r < nr or (r == nr and c < nc) else (nr, nc, r, c)
                    if key not in pre_boundaries:
                        stack.append((nr, nc))
        result.append(comp)
    return result


def _has_internal_boundary(component: set[tuple[int, int]],
                           pre_boundaries: set[tuple[int, int, int, int]]) -> bool:
    """Check if a component contains both endpoints of any pre-boundary."""
    for r1, c1, r2, c2 in pre_boundaries:
        if (r1, c1) in component and (r2, c2) in component:
            return True
    return False


def _boundary_graph_is_bipartite(component: set[tuple[int, int]],
                                 pre_boundaries: set[tuple[int, int, int, int]]) -> bool:
    """Check if the boundary subgraph within a component is bipartite (2-colorable).
    
    A non-bipartite boundary graph means 3+ regions are needed just to satisfy
    the boundary constraints, which is impossible with only 2 remaining regions.
    """
    adj: dict[tuple[int, int], set[tuple[int, int]]] = {}
    for r1, c1, r2, c2 in pre_boundaries:
        if (r1, c1) in component and (r2, c2) in component:
            adj.setdefault((r1, c1), set()).add((r2, c2))
            adj.setdefault((r2, c2), set()).add((r1, c1))
    if not adj:
        return True
    colors: dict[tuple[int, int], int] = {}
    for start in adj:
        if start not in colors:
            colors[start] = 0
            queue: deque[tuple[int, int]] = deque([start])
            while queue:
                cur = queue.popleft()
                for nb in adj.get(cur, set()):
                    if nb not in colors:
                        colors[nb] = 1 - colors[cur]
                        queue.append(nb)
                    elif colors[nb] == colors[cur]:
                        return False
    return True


def _check_compass_dir(self, board: Board, r: int, c: int,
                       dr: int, dc: int, expected: int,
                       region_cells: set[tuple[int, int]]) -> bool:
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

    if self.puzzle.has_rule("rose_window"):
        from src.solver.constraints import _rose_symbol_types
        rose_symbols = _rose_symbol_types(self.puzzle, board)
        if rose_symbols:
            region_syms: set[str] = set()
            for r, c in cells:
                sym = board.cell(r, c).symbol
                if sym is not None:
                    if sym not in rose_symbols:
                        return False
                    if sym in region_syms:
                        return False
                    region_syms.add(sym)

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


def _rose_stop_expanding(self, board: Board, region: set[tuple[int, int]]) -> bool:
    """Return True if this region already has all rose_window symbols and
    no other size/shape constraint requires further expansion."""
    if not self.puzzle.has_rule("rose_window"):
        return False
    if self._has_size_constraint():
        return False
    from src.solver.constraints import _rose_symbol_types
    rose_syms = _rose_symbol_types(self.puzzle, board)
    if not rose_syms or len(rose_syms) < 2:
        return False
    region_syms = {board.cell(r, c).symbol for r, c in region if board.cell(r, c).symbol is not None}
    return region_syms == set(rose_syms)


def _has_size_constraint(self) -> bool:
    """Check if the puzzle has any rule that constrains region size or shape."""
    return (
        self.puzzle.has_rule("precise")
        or self.puzzle.has_rule("area")
        or self.puzzle.has_rule("range")
        or self.puzzle.has_rule("shape_pool")
        or self.puzzle.has_rule("compass")
        or self.puzzle.has_rule("puzzle_piece")
        or self.puzzle.has_rule("block")
        or self.puzzle.has_rule("non_block")
        or self.puzzle.has_rule("solitary")
        or self.puzzle.has_rule("same")
        or self.puzzle.has_rule("different")
    )


def _enumerate_regions(self, board: Board, current: set[tuple[int, int]],
                       frontier: set[tuple[int, int]], unassigned: set[tuple[int, int]],
                       max_area: int, results: list[set[tuple[int, int]]],
                       seed_clue: int | None = None) -> None:
    if len(current) > max_area:
        return

    if hasattr(self, '_enum_budget'):
        self._enum_budget += 1
        if hasattr(self, 'start_time') and self._enum_budget % 2000 == 0:
            import time
            if time.monotonic() - self.start_time > getattr(self, 'timeout', 30.0):
                self._enum_budget = 999999
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
        elif self.puzzle.has_rule("rose_window") and not self._has_size_constraint():
            from src.solver.constraints import _rose_symbol_types as _rst3
            rose_syms = _rst3(self.puzzle, board)
            if rose_syms and len(rose_syms) >= 2:
                if {board.cell(r, c).symbol for r, c in current
                        if board.cell(r, c).symbol is not None} == set(rose_syms):
                    results.append(set(current))
            else:
                if len(current) >= self._min_region_area():
                    results.append(set(current))
        elif len(current) >= self._min_region_area():
            results.append(set(current))

    if len(current) >= max_area:
        return

    if len(results) >= 200:
        return

    if self._rose_stop_expanding(board, current):
        return

    if self.puzzle.has_rule("rose_window") and self._pre_boundaries and self._pre_boundaries_blocking:
        frontier_list = sorted(frontier, key=lambda c: sum(
            1 for dr, dc in [(-1,0),(1,0),(0,-1),(0,1)]
            if 0 <= c[0]+dr < board.height and 0 <= c[1]+dc < board.width
            and ((min(c[0],c[0]+dr), min(c[1],c[1]+dc),
                  max(c[0],c[0]+dr), max(c[1],c[1]+dc))
                in self._pre_boundaries)
        ))
        per_cell_cap = max(1, 200 // max(1, len(frontier_list)))
        for i, cell in enumerate(frontier_list):
            if len(results) >= min(200, per_cell_cap * (i + 1)):
                break
            new_region = current | {cell}
            new_frontier = (frontier - {cell}) | self._frontier({cell}, unassigned - new_region)
            if not self._region_feasible(board, new_region):
                continue
            self._enumerate_regions(board, new_region, new_frontier, unassigned, max_area, results, seed_clue)
    else:
        if self.puzzle.has_rule("rose_window"):
            from src.solver.constraints import _rose_symbol_types as _rst_enum
            rose_syms = _rst_enum(self.puzzle, board)
            if rose_syms:
                current_syms = {board.cell(r, c).symbol for r, c in current
                                if board.cell(r, c).symbol is not None}
                needed = set(rose_syms) - current_syms
                if needed:
                    needed_cells = [(r, c) for r, c in unassigned
                                    if board.cell(r, c).symbol in needed]
                    def _dist_to_needed(cell: tuple[int, int]) -> int:
                        cr, cc = cell
                        return min(abs(cr - nr) + abs(cc - nc)
                                   for nr, nc in needed_cells) if needed_cells else 0
                    frontier_list = sorted(frontier, key=lambda c: (
                        0 if board.cell(c[0], c[1]).symbol in needed else 1,
                        _dist_to_needed(c),
                        c[0], c[1]))
                else:
                    frontier_list = sorted(frontier)
            else:
                frontier_list = sorted(frontier)
        else:
            frontier_list = sorted(frontier)
        for i, cell in enumerate(frontier_list):
            new_region = current | {cell}
            new_frontier = (frontier - {cell}) | self._frontier({cell}, unassigned - new_region)
            if not self._region_feasible(board, new_region):
                continue
            self._enumerate_regions(board, new_region, new_frontier, unassigned, max_area, results, seed_clue)


def _generate_region_candidates(self, board: Board, seed: tuple[int, int],
                                unassigned: set[tuple[int, int]]) -> list[set[tuple[int, int]]]:
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
        elif self.puzzle.has_rule("rose_window") and not self._has_size_constraint():
            from src.solver.constraints import _rose_symbol_types as _rst4, _rose_M as _rM
            rose_syms = _rst4(self.puzzle, board)
            M_val = _rM(self.puzzle, board)
            if rose_syms and M_val > 0:
                total_fillable = len(unassigned)
                for r in range(board.height):
                    for c in range(board.width):
                        if board.cell(r, c).assigned and not board.cell(r, c).blocked:
                            total_fillable += 1
                avg = total_fillable / M_val
                if len(rose_syms) >= 2:
                    if total_fillable > 50:
                        max_area = max(len(rose_syms), int(avg * 1.5))
                    else:
                        max_area = max(len(rose_syms), int(avg * 2.0))
                else:
                    max_area = self._max_region_area()
            else:
                max_area = self._max_region_area()
        else:
            max_area = self._max_region_area()

    initial: set[tuple[int, int]] = {seed}
    frontier = self._frontier(initial, unassigned)

    self._enum_budget = 0
    self._enumerate_regions(board, initial, frontier, unassigned, max_area, results, seed_clue=clue_target)

    if not results:
        if self.puzzle.has_rule("rose_window"):
            from src.solver.constraints import _rose_symbol_types as _rst_fb
            if _rst_fb(self.puzzle, board):
                # {seed} can never be valid for rose_window (missing symbols)
                pass
            else:
                results = [{seed}]
        else:
            results = [{seed}]

    if self.puzzle.has_rule("rose_window"):
        from src.solver.constraints import _rose_symbol_types as _rst_sort
        rose_syms = _rst_sort(self.puzzle, board)
        if rose_syms and len(rose_syms) >= 2:
            def _rose_sort_key(s: set[tuple[int, int]]) -> tuple[int, int]:
                syms = {board.cell(r, c).symbol for r, c in s if board.cell(r, c).symbol is not None}
                return (0 if syms == set(rose_syms) else 1, len(s))
            results.sort(key=_rose_sort_key)
        else:
            # For 1-symbol rose_window, sort by closeness to average region size
            from src.solver.constraints import _rose_M as _rM2
            Mv = _rM2(self.puzzle, board)
            if Mv > 0:
                total = sum(1 for r in range(board.height) for c in range(board.width)
                           if not board.cell(r, c).blocked)
                target = total / Mv
                results.sort(key=lambda s: abs(len(s) - target))
            else:
                results.sort(key=lambda s: len(s))
    elif self.puzzle.has_rule("same"):
        results.sort(key=lambda s: len(s))
    else:
        results.sort(key=lambda s: len(s), reverse=True)
    return results
