# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

《格里米斯的工匠》(The Artisan of Glimmith) — solver & editor for its region-division puzzles. The core task: partition a rectangular grid into connected regions satisfying all on-cell / edge / vertex clue constraints. There are 22 rule types (see `RULE_NAMES` in `src/models/puzzle.py`).

Two solver stacks exist side by side:
- **Python solver** (`src/solver/`) — exact-cover (DLX), rose-window, backtracking.
- **Rust solver** (`rsolver/`) — faster subprocess solver speaking JSON over stdin/stdout.

## Commands

```bash
python src/app.py                       # Qt UI (PySide6)
python -m pytest tests/ -x --tb=short   # full test suite (~365 tests)
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

`SolverRouter` (`src/solver/base.py`) chains solvers in priority order:

```
RustSolver → ExactCoverSolver → RoseSolver → BacktrackSolver → FallbackExactCoverSolver
```

**Key invariant:** the router independently re-verifies every solver's answer via `IndependentValidator` (`src/validation/validator.py`), decoupled from solver-internal rule checks. A wrong answer is logged and the router falls through to the next solver. Any change to rule checking, or a new solver, must preserve this guarantee — a buggy solver can never smuggle a wrong answer through.

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

All 22 rule checkers live in `src/solver/constraints.py` (`RULE_CHECKERS`), one unit-test file per rule in `tests/unit/test_rules/`. `src/solver/validator.py` and `src/validation/validator.py` (`IndependentValidator`) validate final solutions.

### Puzzles, scripts, reference projects

- `puzzles/` — JSON corpus: `official/` (zones A/B/C/Zone1-3), `reference/` (converted from other formats), `user/`, `aiGen/`. `data/polyominoes.json` supplies polyomino data.
- `scripts/` — conversion (`convert_aog_batch.py`, `convert_archive.py`), generation (`gen_ai_puzzles.py`, `generate_polyominoes.py`), benchmarking (`benchmark.py`, `bench_quick.py`).
- `third_party/` — git submodules holding reference solvers used as porting/validation sources (C++ AoG_Solver, Rust aog, JS glimmith-solver, Python TAGSolver, TS shape-helper). Not part of the build.
- `docs/` — architecture, development, testing, and rules guides (in Chinese).

## Testing

- End-to-end tests create a puzzle → solve via the router → validate. `tests/conftest.py` has shared fixtures.
- UI tests use QTest (minimal coverage).
- Puzzle-wide regression: `scripts/verify_puzzles.py` runs the full `default_router()` chain and independently re-validates each solution.
