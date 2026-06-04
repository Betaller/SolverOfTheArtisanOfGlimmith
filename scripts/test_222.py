"""Test solver on puzzles/user/222.json"""
import os, sys, time
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from src.io.puzzle_codec import deserialize
from src.solver.backtrack import BacktrackSolver

puzzle = deserialize('puzzles/user/222.json')
print("Grid: %dx%d" % (puzzle.height, puzzle.width))
print("Rules: %s" % [r.type for r in puzzle.rules])
print("Fillable: %d" % sum(1 for c in puzzle.cells if not c.blocked))
print("Area clues: %s" % [(c.row,c.col,c.number) for c in puzzle.cells if c.number is not None])

t0 = time.monotonic()
solver = BacktrackSolver(puzzle)
sol = solver.solve(timeout=120)
elapsed = time.monotonic() - t0

print("\nSolved: %s" % sol.solved)
print("Time: %.0fms  Steps: %d" % (elapsed*1000, sol.steps_taken))
if sol.solved:
    for r in sol.regions:
        cells_info = [(cr, cc) for cr, cc in sorted(r.cells)]
        print("  Region %d: area=%d  cells=%s" % (r.region_id, r.area, cells_info))
else:
    print("Error: %s" % sol.error_message)
    if sol.regions:
        for r in sol.regions:
            cells_info = [(cr, cc) for cr, cc in sorted(r.cells)]
            print("  Region %d: area=%d  cells=%s" % (r.region_id, r.area, cells_info))
