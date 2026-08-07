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

## results/ 目录规则（每次优化必须遵守）

优化结果统一归档到 `results/` 三个子目录，**禁止在 results/ 根目录散放文件**。

| 目录 | 内容 | 命名规则 | 示例 |
|---|---|---|---|
| `results/bin/` | rsolver 可执行文件（release 构建） | `rsolver-<commit-id>-<platform>`（短 sha，`-` 分隔） | `rsolver-f1cfa16-linux-x86_64` |
| `results/bench/` | `scripts/benchmark_rust_solver.py` 的测试结果 | `<日期>_<commit-id>_<short-message>.txt`（日期 `YYYYMMDD`） | `20260807_c6cb307_opt-v3-bench.txt` |
| `results/tmp/` | 临时测试结果（verify 扫描、根因分析、内存/性能对比等） | 建议 `<日期>_<commit-id>_<short-message>.txt` | `20260806_82c9132_verify-full.txt` |

规则要点：

1. **每次影响求解结果（可解性 / 性能 / 规则语义）的优化提交，必须同时保留：**
   - 产出该结果的 rsolver 可执行文件 → `results/bin/rsolver-<commit-id>-<platform>`，随该提交一起入库；
   - `scripts/benchmark_rust_solver.py` 的测试输出 → `results/bench/<日期>_<commit-id>_<short-message>.txt`，随该提交一起入库。
2. **临时验证 / 分析 / 对比输出**（verify 扫描、根因排查、内存测试等）→ `results/tmp/`，不入库；确认有价值再升格到 bench 或文档。
3. `commit-id` 用产生该结果的提交短 sha（7 位）；`platform` 如 `linux-x86_64`、`windows-x86_64`。
4. 纯文档、无行为变化的重构等不影响求解结果的提交可豁免基准/二进制要求。
5. 归档结果里请注明来源脚本与命令（如 benchmark 脚本会自动打印 header）。

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
