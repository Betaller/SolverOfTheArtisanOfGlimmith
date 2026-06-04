"""Test solver on puzzles/user/111.json"""
import os
import sys
import time
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from src.io.puzzle_codec import deserialize
from src.solver.backtrack import BacktrackSolver

puzzle = deserialize(r'puzzles/user/111.json')
print("Grid: %dx%d" % (puzzle.height, puzzle.width))
print("Rules: %s" % [r.type for r in puzzle.rules])
pool_rule = puzzle.get_rule("shape_pool")
if pool_rule:
    print("Shape pool: %d shapes" % len(pool_rule.params.get("shapes", [])))
print("Fillable cells: %d" % sum(1 for c in puzzle.cells if not c.blocked))
print("Pre-boundaries: %d" % sum(1 for e in puzzle.edges if e.is_boundary))

t0 = time.monotonic()
solver = BacktrackSolver(puzzle)
sol = solver.solve(timeout=60)
elapsed = time.monotonic() - t0

print()
print("Solved: %s" % sol.solved)
print("Time: %.0fms, Steps: %d" % (elapsed * 1000, sol.steps_taken))
print("Regions: %d" % len(sol.regions))
if sol.solved:
    for r in sol.regions:
        print("  Region %d: area=%d, cells=%s" % (r.region_id, r.area, sorted(r.cells)))
else:
    print("Error: %s" % sol.error_message)
