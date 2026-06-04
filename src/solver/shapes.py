from __future__ import annotations

from src.models.board import Shape


def normalize(cells: frozenset[tuple[int, int]]) -> frozenset[tuple[int, int]]:
    if not cells:
        return frozenset()
    min_r = min(r for r, _ in cells)
    min_c = min(c for _, c in cells)
    return frozenset((r - min_r, c - min_c) for r, c in cells)


def rotate_90(cells: frozenset[tuple[int, int]]) -> frozenset[tuple[int, int]]:
    return frozenset((c, -r) for r, c in cells)


def rotate_180(cells: frozenset[tuple[int, int]]) -> frozenset[tuple[int, int]]:
    return frozenset((-r, -c) for r, c in cells)


def rotate_270(cells: frozenset[tuple[int, int]]) -> frozenset[tuple[int, int]]:
    return frozenset((-c, r) for r, c in cells)


def flip_horizontal(cells: frozenset[tuple[int, int]]) -> frozenset[tuple[int, int]]:
    return frozenset((r, -c) for r, c in cells)


def flip_vertical(cells: frozenset[tuple[int, int]]) -> frozenset[tuple[int, int]]:
    return frozenset((-r, c) for r, c in cells)


def all_transformations(cells: frozenset[tuple[int, int]]) -> list[frozenset[tuple[int, int]]]:
    transforms: list[frozenset[tuple[int, int]]] = []
    current = cells
    for _ in range(4):
        transforms.append(current)
        transforms.append(flip_horizontal(current))
        current = rotate_90(current)
    return [normalize(t) for t in transforms]


def canonical_key(cells: frozenset[tuple[int, int]]) -> str:
    normalized_transforms = all_transformations(cells)
    best = min(
        sorted((r, c) for r, c in t)
        for t in normalized_transforms
    )
    return str(best)


def shape_key(shape: Shape) -> str:
    return canonical_key(shape.cells)


def shapes_equal(a: Shape, b: Shape) -> bool:
    if a.area != b.area:
        return False
    return canonical_key(a.cells) == canonical_key(b.cells)


def match_shape_pool(shape: Shape, pool: list[Shape]) -> str | None:
    key = canonical_key(shape.cells)
    for s in pool:
        if canonical_key(s.cells) == key:
            return f"shape_{hash(key) & 0xFFFF}"
    return None


def is_rectangle(shape: Shape) -> bool:
    if not shape.cells:
        return False
    cells = shape.cells
    min_r = min(r for r, _ in cells)
    max_r = max(r for r, _ in cells)
    min_c = min(c for _, c in cells)
    max_c = max(c for _, c in cells)
    expected = (max_r - min_r + 1) * (max_c - min_c + 1)
    return len(cells) == expected


def shape_from_cells(cell_positions: list[tuple[int, int]]) -> Shape:
    return Shape(cells=normalize(frozenset(cell_positions)))


def shape_bitmap(shape: Shape) -> list[list[bool]]:
    h, w = shape.bounding_box
    bitmap: list[list[bool]] = [[False] * w for _ in range(h)]
    for r, c in shape.cells:
        bitmap[r][c] = True
    return bitmap


def shape_from_bitmap(bitmap: list[list[bool]]) -> Shape:
    cells: set[tuple[int, int]] = set()
    for r, row in enumerate(bitmap):
        for c, val in enumerate(row):
            if val:
                cells.add((r, c))
    return Shape(cells=normalize(frozenset(cells)))


def enumerate_polyominoes(n: int) -> list[Shape]:
    if n <= 0:
        return []
    if n == 1:
        return [Shape(cells=frozenset([(0, 0)]))]
    
    smaller = enumerate_polyominoes(n - 1)
    seen: set[str] = set()
    result: list[Shape] = []
    
    for shape in smaller:
        for r, c in shape.cells:
            for dr, dc in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
                nr, nc = r + dr, c + dc
                if (nr, nc) in shape.cells:
                    continue
                new_cells = set(shape.cells)
                new_cells.add((nr, nc))
                norm = normalize(frozenset(new_cells))
                key = canonical_key(norm)
                if key not in seen:
                    seen.add(key)
                    result.append(Shape(cells=norm))
    
    return result
