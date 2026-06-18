from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from src.models.board import Board, Shape
from src.models.puzzle import Puzzle, Rule
from src.solver.backtrack import BacktrackSolver
from src.io.puzzle_codec import serialize
from collections import Counter


BASE = Path(__file__).resolve().parent.parent / "puzzles" / "aiGen"


def make_and_save(category: str, filename: str, puzzle: Puzzle) -> bool:
    path = BASE / category / filename
    solver = BacktrackSolver(puzzle)
    sol = solver.solve(timeout=60)
    if sol.solved:
        areas = Counter(r.area for r in sol.regions)
        print(f"  OK {category}/{filename}: {len(sol.regions)}区域 面积{dict(areas)} {sol.elapsed_ms}ms")
        path.parent.mkdir(parents=True, exist_ok=True)
        serialize(puzzle, str(path))
        return True
    else:
        print(f"  FAIL {category}/{filename}")
        return False


# ============================================================
# 玫瑰窗 (Rose Window)
# ============================================================

# R1: 4x4, N=2(A,B), 每符号8次→M=8, 各区域{A,B}
b = Board(4, 4)
for r in range(4):
    for c in range(4):
        b.cell(r,c).symbol = 'A' if (r+c)%2==0 else 'B'
make_and_save("rose_window", "rose_01_4x4_2sym.json",
              Puzzle.from_board(b, rules=[Rule.rose_window(['A','B'])]))

# R2: 5x5, N=3(A,B,C), 每符号5次→M=5, 4个blocked角
b = Board(5, 5)
for rc in [(0,0),(0,4),(4,0),(4,4)]: b.cell(*rc).blocked = True
syms_map = [(0,1,'A'),(0,2,'B'),(0,3,'C'),(1,0,'B'),(1,1,'C'),(1,2,'A'),(1,3,'B'),(1,4,'C'),
            (2,0,'C'),(2,1,'A'),(2,2,'B'),(2,3,'C'),(2,4,'A'),(3,0,'A'),(3,1,'B'),(3,2,'C'),
            (3,3,'A'),(3,4,'B'),(4,1,'C'),(4,2,'A'),(4,3,'B')]
for r,c,s in syms_map: b.cell(r,c).symbol = s
make_and_save("rose_window", "rose_02_5x5_3sym_blocked.json",
              Puzzle.from_board(b, rules=[Rule.rose_window(['A','B','C'])]))

# R3: 6x4, N=2(A,B), M=6, 中间水平边界
b = Board(6, 4)
for r in range(6):
    for c in range(4):
        b.cell(r,c).symbol = 'A' if (r+c)%2==0 else 'B'
for c in range(4):
    e = b.edge_between(2,c,3,c)
    if e: e.is_boundary = True
make_and_save("rose_window", "rose_03_6x4_2sym_boundary.json",
              Puzzle.from_board(b, rules=[Rule.rose_window(['A','B'])]))

# R4: 6x6, N=3(A,B,C), M=6, 无blocked
b = Board(6, 6)
for r in range(6):
    for c in range(6):
        b.cell(r,c).symbol = ['A','B','C'][(r*3+r//3+c)%3]
make_and_save("rose_window", "rose_04_6x6_3sym.json",
              Puzzle.from_board(b, rules=[Rule.rose_window(['A','B','C'])]))


# ============================================================
# 形状池 (Shape Pool)
# ============================================================

# P1: 4x4, 池=2x2方块, 4区域
block = Shape(cells=frozenset([(0,0),(0,1),(1,0),(1,1)]))
make_and_save("shape_pool", "pool_01_4x4_block.json",
              Puzzle.from_board(Board(4,4), rules=[Rule.shape_pool([block])]))

# P2: 6x6, 池={方块, L形4}, 9区域
L4 = Shape(cells=frozenset([(0,0),(0,1),(0,2),(1,0)]))
make_and_save("shape_pool", "pool_02_6x6_block+L.json",
              Puzzle.from_board(Board(6,6), rules=[Rule.shape_pool([block, L4])]))

# P3: 6x4, 池={水平多米诺, 垂直多米诺}, 12区域
dom_h = Shape(cells=frozenset([(0,0),(0,1)]))
dom_v = Shape(cells=frozenset([(0,0),(1,0)]))
make_and_save("shape_pool", "pool_03_6x4_domino.json",
              Puzzle.from_board(Board(6,4), rules=[Rule.shape_pool([dom_h, dom_v])]))

# P4: 5x5, 池={L形3, 直线3, T形4}, 中间blocked
b = Board(5,5); b.cell(2,2).blocked = True
tri_L = Shape(cells=frozenset([(0,0),(1,0),(1,1)]))
tri_S = Shape(cells=frozenset([(0,0),(0,1),(0,2)]))
T4 = Shape(cells=frozenset([(0,0),(0,1),(0,2),(1,1)]))
make_and_save("shape_pool", "pool_04_5x5_triomino.json",
              Puzzle.from_board(b, rules=[Rule.shape_pool([tri_L, tri_S, T4])]))


# ============================================================
# 玫瑰窗 + 形状池 双约束
# ============================================================

# C1: 4x4, N=2(A,B), M=8, 池={水平多米诺}
b = Board(4,4)
for r in range(4):
    for c in range(4):
        b.cell(r,c).symbol = 'A' if (r+c)%2==0 else 'B'
make_and_save("rose_window+shape_pool", "combo_01_4x4_rose_domino.json",
              Puzzle.from_board(b, rules=[
                  Rule.rose_window(['A','B']),
                  Rule.shape_pool([dom_h]),
              ]))

# C2: 4x4, N=4(A,B,C,D), M=4, 池={L形4, 直线4, 方块4}
b = Board(4,4)
for r in range(4):
    for c in range(4):
        b.cell(r,c).symbol = ['A','B','C','D'][(r*2+r//2+c)%4]
make_and_save("rose_window+shape_pool", "combo_02_4x4_rose+pool.json",
              Puzzle.from_board(b, rules=[
                  Rule.rose_window(['A','B','C','D']),
                  Rule.shape_pool([L4, Shape(cells=frozenset([(0,0),(0,1),(0,2),(0,3)])), block]),
              ]))

# C3: 4x6, N=2(A,B), M=4, 池={2x3方块, 直线6}
b = Board(4,6)
grid = [
    ['A','B',None,None,None,None],
    ['B','A',None,None,None,None],
    [None,None,None,None,'A','B'],
    [None,None,None,None,'B','A'],
]
for r in range(4):
    for c in range(6):
        if grid[r][c]: b.cell(r,c).symbol = grid[r][c]
rect6 = Shape(cells=frozenset([(0,0),(0,1),(0,2),(1,0),(1,1),(1,2)]))
S6 = Shape(cells=frozenset([(0,0),(0,1),(0,2),(0,3),(0,4),(0,5)]))
make_and_save("rose_window+shape_pool", "combo_03_4x6_rose+pool.json",
              Puzzle.from_board(b, rules=[
                  Rule.rose_window(['A','B']),
                  Rule.shape_pool([rect6, S6]),
              ]))

# C4: 6x6, N=3(A,B,C), M=6, 池={2x3方块, 直线6, L形6}, 中间边界
b = Board(6,6)
grid = [
    ['A','B','C',None,None,None],
    ['C','A','B',None,None,None],
    ['B','C','A',None,None,None],
    [None,None,None,'A','B','C'],
    [None,None,None,'C','A','B'],
    [None,None,None,'B','C','A'],
]
for r in range(6):
    for c in range(6):
        if grid[r][c]: b.cell(r,c).symbol = grid[r][c]
for c in range(6):
    e = b.edge_between(2,c,3,c)
    if e: e.is_boundary = True
L6 = Shape(cells=frozenset([(0,0),(0,1),(0,2),(1,0),(2,0),(3,0)]))
make_and_save("rose_window+shape_pool", "combo_04_6x6_rose+pool_boundary.json",
              Puzzle.from_board(b, rules=[
                  Rule.rose_window(['A','B','C']),
                  Rule.shape_pool([rect6, S6, L6]),
              ]))


print(f"\nDone. All puzzles saved under {BASE}")
