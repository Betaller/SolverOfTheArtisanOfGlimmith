# TAGSolver — AGENTS.md

## Run

```powershell
python src/app.py
python -m pytest tests/ -x --tb=short
python scripts/verify_puzzles.py          # all official+user puzzles
```

## Commands

```powershell
ruff check src/ tests/                     # lint
ruff format src/ tests/                    # format (line-length=100)
mypy src/                                  # typecheck (strict)
pre-commit run --all-files                 # CI gate
cd rsolver && cargo build --release        # build Rust solver
```

## Architecture

```
src/app.py                              ← entry point
src/ui/main_window.py                   ← QMainWindow, wires all panels
src/ui/grid_widget.py                   ← custom QPainter grid (cells, edges, boundaries, clues)
src/ui/property_panel.py                ← cell/edge/vertex editor panel
src/ui/tool_palette.py                  ← mode buttons, number/symbol/compass inputs
src/ui/constraint_panel.py              ← 22 rule checkboxes + shape editor
src/solver/backtrack.py                 ← BacktrackSolver (region-by-region DFS)
src/solver/constraints.py               ← 22 RULE_CHECKERS
src/solver/validator.py                 ← SolutionValidator
src/solver/rust_solver.py               ← RustSolver (subprocess → rsolver binary)
src/models/board.py                     ← Board, Cell, Edge, Vertex, Shape, CompassClue
src/models/puzzle.py                    ← Puzzle, Rule, RULE_NAMES
src/io/puzzle_codec.py                  ← JSON serialize/deserialize
rsolver/                                ← Rust solver (puzzle JSON stdin → solution JSON stdout)
```

## Key conventions

- Grid: 2×2~16×16, `Board(height, width)`, cells `(row, col)` 0-indexed
- Edges: `Edge(r1,c1, r2,c2)` between adjacent cells; `is_boundary` flag
- Outer border edges stored as `board.outer_boundaries: list[tuple]`
- 22 rule types defined in `RULE_NAMES` (puzzle.py)
- Shape pool shapes: `Shape(cells=frozenset({(r,c),...}))`, normalized to origin
- Solver respects pre-drawn boundaries (`is_boundary=True` forces different region IDs)
- `properties_panel.board_modified` signal → `grid_widget.update()` for live refresh
- `SolverRouter` chains solvers: RustSolver → ExactCover → Rose → Backtrack → FallbackDLX
- Rust solver communicates via subprocess: puzzle JSON → stdin, solution JSON → stdout

## Solver quirks

- `_board_from_puzzle()` preserves `is_boundary` and `_pre_boundaries` set
- Shape placement optimization used only when no pre-boundaries between fillable cells
- `_enumerate_regions` capped at 200 candidates per seed
- `_check_incremental` enforces mixed/different/heterogeneous/homogeneous/differentiation early
- `_check_global_constraints` final check for shape_pool, rose_window, brick, ring, etc.

## Testing

- 365 tests, all solver unit + integration
- End-to-end tests create puzzle → solve → validate
- `conftest.py` has shared fixtures
- UI tests use QTest (but minimal coverage)
