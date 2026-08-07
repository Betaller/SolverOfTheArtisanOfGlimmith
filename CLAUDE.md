# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

《格里米斯的工匠》(The Artisan of Glimmith) — solver & editor for its region-division puzzles. The core task: partition a rectangular grid into connected regions satisfying all on-cell / edge / vertex clue constraints. There are 22 rule types (see `RULE_NAMES` in `src/models/puzzle.py`).

Solver engine: **Rust solver** (`rsolver/`) — a single subprocess speaking JSON over stdin/stdout. The Python stack (`src/solver/`) holds only the router interface (`base.py`), the Rust subprocess wrapper (`rust_solver.py`), and shared rule/shape infra (`constraints.py` / `shapes.py`); the Python solving algorithms (exact-cover / rose / backtrack) were removed in 2026-08-06 after a corpus-wide evaluation showed they solved nothing the Rust engine can't.

## Commands

```bash
python src/app.py                       # Qt UI (PySide6)
python -m pytest tests/ -x --tb=short   # full test suite (~290 tests)
python scripts/verify_puzzles.py        # verify all puzzles solve (30s timeout each)

ruff check src/ tests/                  # lint (line-length=100)
ruff format src/ tests/                 # format
mypy src/                               # typecheck (strict)
pre-commit run --all-files              # CI gate

cd rsolver && cargo build --release     # build Rust solver (required by RustSolver)
```

Single test: `python -m pytest tests/unit/test_rules/test_rule_08_area.py -x -q`.
Single puzzle debug: `python scripts/verify_puzzles.py --dir puzzles/official/A --timeout 30`.

## Architecture

### Solver routing (Python)

`SolverRouter` (`src/solver/base.py`) chains solvers in priority order. Since 2026-08-06 the chain is Rust-only (the Python exact-cover / rose / backtrack solvers were removed — see `docs/official-puzzles-status.md` §C.0):

```
RustSolver
```

**Key invariant:** the router independently re-verifies every solver's answer via `IndependentValidator` (`src/validation/validator.py`), decoupled from solver-internal rule checks. A wrong answer is logged and the router reports failure. Any change to rule checking, or a new solver, must preserve this guarantee — a buggy solver can never smuggle a wrong answer through.

### Rust solver

`src/solver/rust_solver.py` spawns `rsolver/target/{release,debug}/rsolver[.exe]`; protocol is puzzle JSON → stdin, solution JSON → stdout. `default_router()` constructs `RustSolver()` eagerly, so the app and `verify_puzzles.py` require the binary to be built (`cargo build --release`).

`rsolver/src/solver/mod.rs` dispatches: **aog DFS** (direct port of the C++ reference in `third_party/AoG_Solver`) → **pieces** (DLX exact cover for shape_pool / area clues / constrained compass) → **backtrack** (region-by-region DFS). The aog solver's internal checks are treated as authoritative (`build_solution_trusted`) — no re-validation in Rust.

Recent `rsolver` work ports C++ pruning into the aog DFS: ring T-junction prune (禁T字), slash-distance prune (`dfs.cpp` lines 1260-1306), multi-char rose symbols, enlarged shape/stack arrays. The Rust backtrack solver locally enforces ring (no 3-way) and brick (no 4-way) junctions.

### Domain model & JSON

- Models in `src/models/`: `Board` / `Cell` / `Edge` / `Vertex` / `Shape` / `CompassClue` (`board.py`), `Puzzle` / `Rule` / `RULE_NAMES` (`puzzle.py`), `Solution` (`solution.py`).
- `src/io/puzzle_codec.py` is the single JSON serializer/deserializer shared by UI, Rust subprocess, and scripts. Top-level fields: `grid{height,width}`, `cells[]`, `edges[]`, `vertices[]`, `rules[]`, `shape_pool[]`, `outer_boundaries[]`.

Conventions:
- Grid is 2×2 ~ 16×16; cells are `(row, col)` 0-indexed.
- Edges `Edge(r1,c1, r2,c2)` between adjacent cells; `is_boundary=True` forces different region IDs; the outer border lives separately in `board.outer_boundaries`.
- Pool shapes are `Shape(cells=frozenset({(r,c), ...}))` normalized to origin; the canonical form is the lexicographically smallest of the 8 rotations/reflections (`src/solver/shapes.py`).
- Solvers must respect pre-drawn boundaries.

### Rule checkers

All 22 rule checkers live in `src/solver/constraints.py` (`RULE_CHECKERS`), one unit-test file per rule in `tests/unit/test_rules/`. `src/validation/validator.py` (`IndependentValidator`) validates final solutions (the only validation entry point; `src/solver/validator.py` was removed with the Python solver stack).

### Puzzles, scripts, reference projects

- `puzzles/` — JSON corpus: `official/` (zones A/B/C/Zone1-3), `reference/` (converted from other formats), `user/`, `aiGen/`. `data/polyominoes.json` supplies polyomino data.
- `scripts/` — conversion (`convert_aog_batch.py`, `convert_archive.py`), generation (`gen_ai_puzzles.py`, `generate_polyominoes.py`), benchmarking (`benchmark.py`, `bench_quick.py`).
- `third_party/` — git submodules holding reference solvers used as porting/validation sources (C++ AoG_Solver, Rust aog, JS glimmith-solver, Python TAGSolver, TS shape-helper). Not part of the build.
- `docs/` — architecture, development, testing, and rules guides (in Chinese). `docs/official-puzzles-status.md` tracks the official-corpus solve status / DIFF / UNSOLVED analysis.

## 文档软门禁（Soft Gate）

**门禁总则：每次修改代码，都必须同步更新相关文档。** 只要改了求解器 / 转换脚本 /
规则校验器 / 规则语义 / 领域模型 / JSON 协议中的任何行为或结构，就要在**同一个提交**
里带上文档改动；改了文档也一样要顺带检查代码是否需要同步。文档没跟上即视为未完成。

相关文档按修改范围对应（改动涉及哪个就更新哪个）：

| 改动范围 | 必须同步的文档 |
|---|---|
| `rsolver/src/**`（求解算法 / 数据结构 / 路由 / 规则实现） | `docs/rust-solver/**` 中对应的那几篇（见下方对照） |
| `src/**` Python 求解器 / `scripts/**` 转换与生成 | `docs/architecture.md` / `docs/rules-guide.md` / `docs/重构/**` |
| 规则语义 / 22 条规则定义 | `docs/rules-guide.md` |
| 官方题扫描 / DIFF / UNSOLVED 变化 / 求解能力变化 | `docs/official-puzzles-status.md` |
| 外部可观察行为（JSON 格式、CLI、路由顺序） | `README.md` |

`docs/rust-solver/` 系列与 `rsolver/src/` 的对照（源码改动必查对应篇）：

- `solver/mod.rs`（路由 / 答案构建） → `01-总体架构.md`
- `types.rs` / `grid.rs` / `solver/aog/types.rs`（模型 / 位域） → `02-数据结构.md`
- 任何规则实现 → `03-规则与代码映射.md`
- `solver/aog/**` → `04-aog求解器.md`
- `solver/pieces.rs` / `dlx.rs` → `05-pieces求解器.md`
- `solver/backtrack.rs` → `06-backtrack求解器.md`
- `solver/rose/**` → `07-rose求解器.md`
- `solver/validate.rs`（完整独立验证器；原 `constraints.rs` 已于 2026-08-06 删除，逻辑并入） → `08-验证与约束检查.md`
- 拼块（puzzle_piece / shape_pool）优化 → `10-拼块优化方向.md`
- 拼块 + 玫瑰窗混合优化 → `docs/优化/09-rose-puzzle-piece优化调研.md`
- 边界推演 / 专用求解器 / 规则组合优化 → `docs/优化/10-专用求解器方案.md`

### 常见陷阱：边数组索引

`h_edges[r][c]` 维度是 `[h][w-1]`（每行 `w` 个格之间有 `w-1` 条水平边），
`v_edges[r][c]` 维度是 `[h-1][w]`（每列 `h` 个格之间有 `h-1` 条垂直边）。
**访问 `h_edges` 时，列索引必须 `< w-1`（即 `vc + 1 < w`）；访问 `v_edges` 时，
行索引必须 `< h-1`（即 `vr + 1 < h`）**。用 `vc < w` 或 `vr < h` 做边界检查
会越界 panic（exit code 101）。详见 `docs/优化/10-专用求解器方案.md` 附录 B.1。

**官方题准则：官方解是唯一解。** 对求解器 / 转换脚本 / 规则校验器 / 规则语义的**任何优化**，
除上述文档同步外，还必须：

1. 更新 `docs/official-puzzles-status.md`（进度数字、DIFF/UNSOLVED 变化、结论）。
2. 跑通 `pytest`、`cargo test` 与相关 `verify_puzzles.py` 片段，把结果记入该文档。
3. **基准评估结果随提交入库**：影响求解结果（可解性 / 性能 / 规则语义）的提交，必须把对应
   基准 / 全量扫描输出存为 `results/YYYYMMDD_<short-sha|描述>.txt` 并**随该提交一起入库**
   （不允许只留在 /tmp）。同时把产出该结果的 `rsolver` 二进制存为
   `results/bin/rsolver-<short-sha>.<平台>`（如 `rsolver-f1cfa16.linux-x86_64`），保证结果可复现。
   纯文档、无行为变化的重构等不影响求解结果的提交可豁免。

不满足即视为未完成。全量扫描方式与脚本见该文档 §2。

## Testing

- End-to-end tests create a puzzle → solve via the router → validate. `tests/conftest.py` has shared fixtures.
- UI tests use QTest (minimal coverage).
- Puzzle-wide regression: `scripts/verify_puzzles.py` runs the full `default_router()` chain and independently re-validates each solution.
