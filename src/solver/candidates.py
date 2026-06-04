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


def _enumerate_regions(self, board: Board, current: set[tuple[int, int]],
                       frontier: set[tuple[int, int]], unassigned: set[tuple[int, int]],
                       max_area: int, results: list[set[tuple[int, int]]],
                       seed_clue: int | None = None) -> None:
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
