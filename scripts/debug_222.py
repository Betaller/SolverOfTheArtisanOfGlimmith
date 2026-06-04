"""Debug 222.json solution rendering"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from src.io.puzzle_codec import deserialize
from src.solver.backtrack import BacktrackSolver

puzzle = deserialize('puzzles/user/222.json')
solver = BacktrackSolver(puzzle)
sol = solver.solve(timeout=30)

grid = {}
for r in range(1, 5):
    for c in range(1, 8):
        grid[(r,c)] = None

for reg in sol.regions:
    for (r,c) in reg.cells:
        grid[(r,c)] = reg.region_id

print("Region map (rows 1-4, cols 1-7):")
print("      c1  c2  c3  c4  c5  c6  c7")
for r in range(1, 5):
    row_str = "r%d:" % r
    for c in range(1, 8):
        rid = grid.get((r,c))
        row_str += " R%-2d" % rid if rid is not None else " X "
    print(row_str)

print("\nBoundaries needed:")
for r in range(1, 5):
    for c in range(1, 8):
        rid = grid.get((r,c))
        for dr, dc, name in [(-1,0,"上"), (1,0,"下"), (0,-1,"左"), (0,1,"右")]:
            nr, nc = r+dr, c+dc
            nrid = grid.get((nr,nc))
            if nrid is not None and rid != nrid:
                print("  (%d,%d)[R%d] %s -> (%d,%d)[R%d] BORDER" % (r,c,rid,name,nr,nc,nrid))
