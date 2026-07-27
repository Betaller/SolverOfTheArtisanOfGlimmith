"""预计算所有自由多联骨牌 (free polyominoes) 1~12 格，保存为 JSON。

从 src/solver/shapes.py 的 enumerate_polyominoes 生成，
文件 data/polyominoes.json 格式: { "4": [[[0,0],[0,1],...], ...], ... }
"""

from __future__ import annotations

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from src.solver.shapes import enumerate_polyominoes


def generate(max_size: int = 12) -> dict[int, list[list[list[int]]]]:
    """Generate all free polyominoes 1..max_size."""
    result: dict[int, list[list[list[int]]]] = {}
    for n in range(1, max_size + 1):
        polys = enumerate_polyominoes(n)
        shapes = [[[r, c] for r, c in sorted(shape.cells)] for shape in polys]
        result[str(n)] = shapes
        print(f"  size {n}: {len(shapes)} shapes")
    return result


def main():
    out_dir = os.path.join(os.path.dirname(__file__), "..", "data")
    os.makedirs(out_dir, exist_ok=True)
    out_path = os.path.join(out_dir, "polyominoes.json")

    print("Generating polyominoes 1–12...")
    data = generate(12)

    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(data, f)

    total = sum(len(v) for v in data.values())
    print(f"\nSaved {total} shapes to {out_path}")


if __name__ == "__main__":
    main()
