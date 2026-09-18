#!/usr/bin/env python3
"""Benchmark the Rust solver against puzzle corpora.

Features (all optional):
  --timeout 20       per-puzzle timeout (default 20 s).  Threaded into the Rust
                     search via RSOLVER_TIMEOUT_MS (was hardcoded 30s in Rust).
  -j N / --jobs N    parallel workers (0 = cpu_count)
  --batch N          reuse one rsolver subprocess for every N puzzles (default 1)
  --out JSONL        append per-puzzle records to a JSONL file
  --rules RULE       only test puzzles containing this rule type
  --zone ZONE        only test puzzles in this zone (Zone1 / A / ...)
  --resume FILE      skip puzzles already PASS in a previous run (not a
                     regression check — it cannot detect PASS→FAIL)
  --baseline JSONL   REGRESSION mode: compare against a previous --out JSONL.
                     Re-runs baseline-PASS (detect REGRESSION) + fast baseline-
                     FAIL (detect NEW) puzzles; known-slow baseline-FAILs are
                     probed last @ min(timeout,10)s unless --skip-slow.  Exit 2
                     on regression, 1 on failure, 0 clean.
  --skip-slow        with --baseline: skip known-slow baseline-FAILs entirely
  --skip-slow-threshold MS  elapsed_ms at/above which a baseline FAIL is "slow"
                            (default 15000)
  --adaptive-j       auto-reduce concurrency on OOM (exit -9) or timeout spikes
  --retry-timeouts   retry timed-out puzzles with original-j//4 and 2x timeout

Two-tier workflow:
  Quick (daily):   --baseline results/bench/latest.jsonl --timeout 40 -j 8 --skip-slow
                   (same timeout as the baseline → same-oracle REGRESSION detection;
                    --skip-slow drops known-slow FAILs for speed; parallel-load
                    noise on borderline PASS puzzles can be confirmed via
                    --retry-timeouts or a solo rerun)
  Full  (pre-commit):     --timeout 40 -j 8 --out results/bench/<date>_<sha>.jsonl

Usage:
  python scripts/benchmark_rust_solver.py
  python scripts/benchmark_rust_solver.py --dir puzzles/official/Zone1 --timeout 20 -j 4
  python scripts/benchmark_rust_solver.py --baseline results/bench/latest.jsonl --timeout 10 -j 8
"""

from __future__ import annotations

import argparse
import concurrent.futures
import glob
import json
import os
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

sys.path.insert(0, ".")

from src.io.puzzle_codec import dict_to_puzzle
from src.solver.rust_solver import RustSolver
from src.validation.official_answer import matches_official_answer
from src.validation.validator import IndependentValidator, solution_to_board

# ── data types ────────────────────────────────────────────────────────────


@dataclass
class PuzzleResult:
    name: str
    path: str = ""
    solved: bool = False
    validated: bool = False
    elapsed_ms: int = 0
    error: str | None = None
    solver: str = ""
    matches_official: bool | None = None
    # Per-module attempt trace (doc 23): list of {solver,status,elapsed_ms,note}.
    # Empty for load errors / rule-less puzzles / old binaries without the field.
    attempts: list[dict[str, object]] = field(default_factory=list)


# ── helpers ───────────────────────────────────────────────────────────────


def _discover_files(dir: str, rule_filter: str | None = None) -> list[str]:
    files = sorted(
        f
        for f in glob.glob(f"{dir}/**/*.json", recursive=True)
        if not Path(f).name.startswith("_")
        and not any(part.endswith("-answer") for part in Path(f).parts)
    )
    if not rule_filter:
        return files
    kept: list[str] = []
    for f in files:
        try:
            with open(f, encoding="utf-8") as fh:
                data = json.load(fh)
            if any(r.get("type") == rule_filter for r in data.get("rules", [])):
                kept.append(f)
        except Exception:
            continue
    return kept


def _resume_name_from_line(line: str) -> str | None:
    """Return the puzzle name from a previous-benchmark PASS line, else None."""
    if line.startswith("[") and "\tPASS " in line.replace("  ", "\t"):
        try:
            name = line.split()[-3]  # "name.json"
            if name.endswith(".json"):
                return name
        except (IndexError, ValueError):
            return None
    elif line.startswith("[") and " PASS " in line:
        try:
            for p in line.split():
                if p.endswith(".json"):
                    return p
        except (IndexError, ValueError):
            return None
    return None


def _load_resume_set(path: str) -> set[str]:
    """Read a previous benchmark output and return the set of *passed* puzzle names."""
    passed: set[str] = set()
    if not os.path.isfile(path):
        return passed
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            name = _resume_name_from_line(line)
            if name:
                passed.add(name)
    return passed


# ── baseline regression mode ──────────────────────────────────────────────


def _baseline_key(zone: str, name: str) -> str:
    """Stable, CWD/`--dir`-independent key for matching a puzzle across runs.

    ``file`` paths differ when the baseline was generated with a different
    `--dir` or CWD; ``zone + "/" + name`` is unique (zone disambiguates same
    basenames across zones) and is recorded verbatim in the JSONL.
    """
    return f"{zone}/{name}"


def _load_baseline(path: str) -> dict[str, dict[str, Any]]:
    """Load a previous run's JSONL `--out` file into ``{key: record}``.

    Keyed by :func:`_baseline_key` (``zone/name``).  When the same key appears
    on multiple lines (e.g. a retry appended a corrected record after a stale
    one), the **last** record wins — downstream consumers of `--out` MUST apply
    last-wins semantics too (unlike ``_load_resume_set`` which takes any PASS).
    Returns ``{}`` if the file is missing.
    """
    baseline: dict[str, dict[str, Any]] = {}
    if not os.path.isfile(path):
        return baseline
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            zone = rec.get("zone") or "?"
            name = rec.get("name") or Path(rec.get("file", "")).name or "?"
            baseline[_baseline_key(zone, name)] = rec
    return baseline


def _classify_result(
    key: str,
    r: PuzzleResult,
    baseline: dict[str, dict[str, Any]],
    regressions: list[str],
    news: list[str],
    new_files: list[str],
) -> None:
    prev = baseline.get(key)
    if prev is None:
        new_files.append(key)
        return
    prev_pass = prev.get("status") == "PASS"
    now_pass = r.solved and r.validated
    if prev_pass and not now_pass:
        regressions.append(key)
    elif not prev_pass and now_pass:
        news.append(key)


def _print_regression_summary(
    current: dict[str, PuzzleResult],
    baseline: dict[str, dict[str, Any]],
    skipped_slow: list[str],
) -> tuple[list[str], list[str], list[str]]:
    """Compare current results against the baseline.

    Prints REGRESSION (baseline PASS → now FAIL) / NEW (baseline FAIL → now
    PASS) / NEW FILE (absent from baseline) tables and a one-line tally.
    Returns ``(regressions, news, new_files)`` key lists for exit-code logic.
    """
    regressions: list[str] = []
    news: list[str] = []
    new_files: list[str] = []

    for key, r in current.items():
        _classify_result(key, r, baseline, regressions, news, new_files)

    print(
        f"\n回归对比: REGRESSION={len(regressions)}  NEW={len(news)}  "
        f"NEW FILE={len(new_files)}  SKIPPED-SLOW={len(skipped_slow)}"
    )
    if regressions:
        print("REGRESSION (基线 PASS → 现 FAIL):")
        for k in sorted(regressions):
            r = current[k]
            print(f"  {k}: {r.error or 'failed'}")
    if news:
        print("NEW (基线 FAIL → 现 PASS):")
        for k in sorted(news):
            print(f"  {k}")
    if new_files:
        print("NEW FILE (基线无此题):")
        for k in sorted(new_files)[:20]:
            print(f"  {k}")
        if len(new_files) > 20:
            print(f"  ...及另外 {len(new_files) - 20} 题")
    return regressions, news, new_files


def _zone(path: str, root_dir: str) -> str:
    """Extract zone name.

    For puzzle paths like ``official/Zone1/type/name.json`` this returns
    ``Zone1``.  Falls back to the immediate parent directory name.
    """
    try:
        rel = str(Path(path).relative_to(root_dir))
    except ValueError:
        return Path(path).parent.name or "?"
    parts = rel.replace("\\", "/").split("/")
    if len(parts) >= 3 and parts[0] == "official":
        return parts[1]  # official / Zone1 / type / name.json
    if len(parts) >= 2:
        return parts[0]  # top-level folder
    return parts[0] if parts else "?"


# ── single / batch solving ────────────────────────────────────────────────


def _attempts_to_dicts(attempts: list) -> list[dict[str, object]]:
    """Serialize a `Solution.attempts` trace to plain dicts for JSONL (doc 23).

    Each `SolverAttempt` carries an `AttemptStatus` enum; store its `.value`
    string so the JSONL stays human-readable and stable across renames.
    """
    out: list[dict[str, object]] = []
    for a in attempts or []:
        status = a.status.value if hasattr(a.status, "value") else str(a.status)
        out.append(
            {
                "solver": a.solver,
                "status": status,
                "elapsed_ms": a.elapsed_ms,
                "note": a.note,
            }
        )
    return out


def _attempt_chain_str(attempts: list[dict[str, object]]) -> str:
    """Compact one-line rendering of the attempt chain for the console row:
    `aog:timeout→rose:success`.  Returns '' when there is no trace."""
    if not attempts:
        return ""
    return "→".join(
        f"{a['solver']}:{a['status']}" for a in attempts if a.get("status") != "not_attempted"
    )


def _validate(puzzle: object, out: dict, path: str | None = None) -> dict[str, Any]:
    """Independent re-validation.  Returns a dict with keys solved, validated, error, solver."""
    solver = out.get("solver", "")
    if not out.get("solved"):
        return {
            "solved": False,
            "validated": False,
            "solver": solver,
            "error": out.get("error_message", "no solution")[:300],
        }
    from types import SimpleNamespace

    regions = []
    for rd in out.get("regions", []):
        cells = [(c[0], c[1]) for c in rd.get("cells", [])]
        regions.append(
            SimpleNamespace(
                region_id=rd["region_id"],
                cells=cells,
                area=rd.get("area", len(cells)),
                shape=[(s[0], s[1]) for s in rd.get("shape", [])],
            )
        )
    sol = SimpleNamespace(board=None, regions=regions, rule_results={})
    board = solution_to_board(puzzle, sol)
    result = IndependentValidator().validate(puzzle, board)
    r: dict[str, Any] = {
        "solved": result.solved,
        "validated": result.solved,
        "solver": solver,
        "error": "; ".join(result.errors[:3]) if not result.solved else None,
    }
    if result.solved and path is not None:
        r["matches_official"] = matches_official_answer(path, [reg.cells for reg in regions])
    return r


def solve_one(path: str, timeout: float, solver: RustSolver) -> PuzzleResult:
    name = Path(path).name
    try:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
        puzzle = dict_to_puzzle(data)
    except Exception as e:
        return PuzzleResult(name=name, path=path, error=f"load error: {e}")

    if not puzzle.rules:
        return PuzzleResult(name=name, path=path, solved=True, validated=True)

    try:
        solution = solver.solve(puzzle, timeout=timeout)
    except Exception as e:
        return PuzzleResult(
            name=name, path=path, elapsed_ms=int(timeout * 1000), error=f"solver error: {e}"
        )

    regions_out = []
    for reg in solution.regions or []:
        shape_cells = list(reg.shape.cells) if hasattr(reg.shape, "cells") else (reg.shape or [])
        regions_out.append(
            {"region_id": reg.region_id, "cells": reg.cells, "area": reg.area, "shape": shape_cells}
        )
    r = _validate(
        puzzle,
        {
            "solved": solution.solved,
            "solver": solution.solver or "",
            "error_message": solution.error_message,
            "regions": regions_out,
            "elapsed_ms": solution.elapsed_ms,
        },
        path,
    )
    return PuzzleResult(
        name=name,
        path=path,
        solved=r["solved"],
        validated=r["validated"],
        elapsed_ms=solution.elapsed_ms,
        error=r.get("error"),
        solver=r.get("solver", ""),
        matches_official=r.get("matches_official"),
        attempts=_attempts_to_dicts(solution.attempts),
    )


def _result_from_solution(path: str, puzzle: object, sol: object) -> PuzzleResult:
    """Build a benchmark ``PuzzleResult`` from one solver solution (batch mode)."""
    regions_out = []
    for reg in sol.regions or []:
        shape_cells = list(reg.shape.cells) if hasattr(reg.shape, "cells") else (reg.shape or [])
        regions_out.append(
            {
                "region_id": reg.region_id,
                "cells": reg.cells,
                "area": reg.area,
                "shape": shape_cells,
            }
        )
    r = _validate(
        puzzle,
        {
            "solved": sol.solved,
            "solver": sol.solver or "",
            "error_message": sol.error_message,
            "regions": regions_out,
            "elapsed_ms": sol.elapsed_ms,
        },
        path,
    )
    return PuzzleResult(
        name=Path(path).name,
        path=path,
        solved=r["solved"],
        validated=r["validated"],
        elapsed_ms=sol.elapsed_ms,
        error=r.get("error"),
        solver=r.get("solver", ""),
        matches_official=r.get("matches_official"),
        attempts=_attempts_to_dicts(sol.attempts),
    )


def solve_batch(
    paths: list[str], timeout: float, solver: RustSolver
) -> list[tuple[str, PuzzleResult]]:
    results: list[tuple[str, PuzzleResult]] = []
    pending: list[tuple[str, object]] = []  # (path, puzzle)
    for p in paths:
        name = Path(p).name
        try:
            with open(p, encoding="utf-8") as f:
                data = json.load(f)
            puzzle = dict_to_puzzle(data)
        except Exception as e:
            results.append((p, PuzzleResult(name=name, path=p, error=f"load error: {e}")))
            continue
        if not puzzle.rules:
            results.append((p, PuzzleResult(name=name, path=p, solved=True, validated=True)))
            continue
        pending.append((p, puzzle))

    if not pending:
        return results

    try:
        solutions = solver.solve_batch([p for _, p in pending], timeout=timeout)
    except Exception as e:
        for p, _ in pending:
            results.append((p, PuzzleResult(name=Path(p).name, path=p, error=f"batch error: {e}")))
        return results

    for (p, puzzle), sol in zip(pending, solutions, strict=False):
        results.append((p, _result_from_solution(p, puzzle, sol)))
    return results


# ── main ───────────────────────────────────────────────────────────────────


def _build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Benchmark Rust solver on puzzles",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""Examples:
  %(prog)s --dir puzzles/official --timeout 20 -j 8
  %(prog)s --baseline results/bench/latest.jsonl --timeout 10 -j 8   # quick tier
  %(prog)s --rules block --timeout 30
  %(prog)s --zone Zone3 --timeout 10 -j 8
  %(prog)s --batch 100 -j 1         # batch mode, sequential""",
    )
    parser.add_argument("--dir", default="puzzles/official")
    parser.add_argument(
        "--timeout", type=float, default=20.0, help="per-puzzle timeout in seconds (default 20)"
    )
    parser.add_argument("-j", "--jobs", type=int, default=0, help="parallel workers (0=cpu_count)")
    parser.add_argument(
        "--batch",
        type=int,
        default=1,
        help="batch size for rsolver --batch reuse (default 1 = one subprocess "
        "per puzzle, safest for parallel regression). "
        "Larger N (e.g. 50) reuses one subprocess per N puzzles, cutting spawn "
        "overhead ~90%% (1258 → ~25 subprocesses) — good for sequential runs "
        "(-j 1) but WARNING: with -j>1, each worker holds a long-lived batch "
        "subprocess, so memory pressure compounds and OOM (exit -9) can occur "
        "on memory-heavy puzzles. Each line is time-bounded by RSOLVER_TIMEOUT_MS.",
    )
    parser.add_argument("--resume", help="skip puzzles already PASS in this file")
    parser.add_argument(
        "--adaptive-j", action="store_true", help="auto-reduce -j on OOM / timeout spikes"
    )
    parser.add_argument("--rules", help="only test puzzles containing this rule type")
    parser.add_argument("--zone", help="only test puzzles in this zone (e.g. Zone1, A)")
    parser.add_argument("--out", help="append JSONL records to this file")
    parser.add_argument("--summary-only", action="store_true", help="only print final summary")
    parser.add_argument(
        "--retry-timeouts",
        action="store_true",
        help="retry timed-out puzzles with original -j//4 and 2x timeout (once)",
    )
    parser.add_argument(
        "--baseline",
        metavar="JSONL",
        help="regression mode: compare against this previous run's --out JSONL. "
        "Re-runs baseline-PASS puzzles (detect REGRESSION) and fast baseline-FAIL "
        "puzzles (detect NEW solves); known-slow baseline-FAILs are probed last at "
        "min(timeout,10)s unless --skip-slow.",
    )
    parser.add_argument(
        "--skip-slow",
        action="store_true",
        help="with --baseline: skip known-slow baseline-FAILs entirely (fastest). "
        "Default probes them last to catch new solves.",
    )
    parser.add_argument(
        "--skip-slow-threshold",
        type=int,
        default=15000,
        metavar="MS",
        help="baseline elapsed_ms at/above which a FAIL is 'known-slow' (default 15000)",
    )
    return parser


@dataclass
class _RunState:
    """Mutable accumulator threaded through the benchmark run."""

    passed: int = 0
    failed: list[PuzzleResult] = field(default_factory=list)
    by_zone: dict[str, tuple[int, int]] = field(default_factory=dict)
    current_results: dict[str, PuzzleResult] = field(default_factory=dict)
    oom_streak: int = 0
    timeout_streak: int = 0


def _attempt_status(r: PuzzleResult) -> tuple[str, bool]:
    diff = r.solved and r.validated and r.matches_official is False
    status = "DIFF" if diff else ("PASS" if r.solved and r.validated else "FAIL")
    return status, diff


def _record_attempt(state: _RunState, r: PuzzleResult, diff: bool) -> None:
    if diff:
        r.error = (r.error or "") + " 解与官方题解不一致"
        state.failed.append(r)
    elif r.solved and r.validated:
        state.passed += 1
    else:
        state.failed.append(r)


def _track_streak(state: _RunState, r: PuzzleResult) -> None:
    """Update oom/timeout streaks from a result's error text (for --adaptive-j)."""
    err = r.error or ""
    if "exit -9" in err:
        state.oom_streak += 1
        state.timeout_streak = 0
    elif "timeout" in err.lower():
        state.timeout_streak += 1
        state.oom_streak = 0
    else:
        state.oom_streak = 0
        state.timeout_streak = 0


def _emit_console(
    state: _RunState,
    args: argparse.Namespace,
    r: PuzzleResult,
    zone: str,
    status: str,
    total_files: int,
) -> None:
    if args.summary_only:
        return
    chain = _attempt_chain_str(r.attempts)
    via = chain or (r.solver or "-")
    print(
        f"[{state.passed + len(state.failed):>4}/{total_files}] {status:<4} {zone:<8} "
        f"{r.name:<24} via={via:<24} {r.elapsed_ms:>6}ms"
        f"{'  ' + r.error if r.error else ''}"
    )


def _write_jsonl(
    args: argparse.Namespace, path: str, r: PuzzleResult, zone: str, status: str
) -> None:
    if not args.out:
        return
    with open(args.out, "a", encoding="utf-8") as fh:
        fh.write(
            json.dumps(
                {
                    "file": path,
                    "name": r.name,
                    "zone": zone,
                    "status": status,
                    "solved": r.solved,
                    "validated": r.validated,
                    "elapsed_ms": r.elapsed_ms,
                    "solver": r.solver,
                    "error": r.error,
                    "matches_official": r.matches_official,
                    "attempts": r.attempts,
                },
                ensure_ascii=False,
            )
            + "\n"
        )


def _report(
    state: _RunState, args: argparse.Namespace, path: str, r: PuzzleResult, total_files: int
) -> None:
    zone = _zone(path, args.dir)
    zt, zp = state.by_zone.get(zone, (0, 0))
    state.by_zone[zone] = (zt + 1, zp + (1 if r.solved else 0))
    state.current_results[_baseline_key(zone, r.name)] = r
    status, diff = _attempt_status(r)
    _record_attempt(state, r, diff)
    _track_streak(state, r)
    _emit_console(state, args, r, zone, status, total_files)
    _write_jsonl(args, path, r, zone, status)


def _undo(state: _RunState, args: argparse.Namespace, old_r: PuzzleResult) -> None:
    """Reverse the accounting ``report`` applied for ``old_r`` before re-reporting."""
    zone = _zone(old_r.path, args.dir)
    zt, zp = state.by_zone.get(zone, (0, 0))
    state.by_zone[zone] = (zt - 1, zp - (1 if old_r.solved and old_r.validated else 0))
    if old_r.solved and old_r.validated:
        state.passed -= 1


def _partition_baseline_files(
    args: argparse.Namespace, files: list[str], baseline: dict[str, dict[str, Any]]
) -> tuple[list[str], list[str], list[str]]:
    """Split the run set into main / known-slow buckets for --baseline mode.

    Returns ``(run_files, slow_files, slow_skipped)``.  Auto-defaults ``--out``
    so a fresh baseline JSONL is always produced.
    """
    slow_files: list[str] = []
    slow_skipped: list[str] = []
    if not args.baseline:
        return files, slow_files, slow_skipped
    if not baseline:
        print(f"warning: --baseline {args.baseline} empty/unreadable; running full.")
        return files, slow_files, slow_skipped
    if not args.out:
        args.out = f"results/tmp/{time.strftime('%Y%m%d')}_regression.jsonl"
    main_files: list[str] = []
    thr = args.skip_slow_threshold
    for f in files:
        zone = _zone(f, args.dir)
        key = _baseline_key(zone, Path(f).name)
        prev = baseline.get(key)
        if prev is None:
            main_files.append(f)  # new file — run full timeout
        elif prev.get("status") == "PASS":
            main_files.append(f)  # regression check
        elif prev.get("elapsed_ms", 0) >= thr:
            slow_files.append(f)  # known-slow — defer
            if args.skip_slow:
                slow_skipped.append(key)
        else:
            main_files.append(f)  # fast FAIL — new-solve check
    n_main, n_slow = len(main_files), len(slow_files)
    if args.skip_slow:
        print(f"baseline: {n_main} run, {n_slow} skipped (--skip-slow)")
        return main_files, slow_files, slow_skipped
    print(f"baseline: {n_main} main + {n_slow} known-slow (probed last @ min(timeout,10)s)")
    return main_files + slow_files, slow_files, slow_skipped


def _apply_zone_filter(args: argparse.Namespace, files: list[str]) -> list[str]:
    if args.zone:
        files = [f for f in files if _zone(f, args.dir) == args.zone]
        if not files:
            print(f"no puzzles in zone {args.zone} under {args.dir}/")
            sys.exit(1)
    return files


def _apply_resume(args: argparse.Namespace, files: list[str]) -> tuple[list[str], set[str]]:
    skip_names: set[str] = set()
    if args.resume:
        skip_names = _load_resume_set(args.resume)
        if skip_names:
            before = len(files)
            files = [f for f in files if Path(f).name not in skip_names]
            print(f"resume: skipped {before - len(files)} already-passed, {len(files)} remaining")
    return files, skip_names


def _adaptive_throttle(
    state: _RunState, jobs: int, pool: concurrent.futures.ThreadPoolExecutor
) -> int:
    """Reduce pool concurrency on OOM / timeout streaks; returns the new `jobs`."""
    if state.oom_streak >= 3 and jobs > 1:
        jobs = max(1, jobs // 2)
        print(f"⚠ OOM streak {state.oom_streak}, reducing concurrency to j={jobs}")
        state.oom_streak = 0
        pool._max_workers = jobs
    elif state.timeout_streak >= 5 and jobs > 2:
        jobs = max(1, jobs - 2)
        print(f"⚠ timeout streak {state.timeout_streak}, reducing concurrency to j={jobs}")
        state.timeout_streak = 0
    return jobs


def _file_timeout(path: str, slow_set: set[str], timeout: float, slow_timeout: float) -> float:
    return slow_timeout if path in slow_set else timeout


def _future_result(fut: concurrent.futures.Future, path: str) -> PuzzleResult:
    try:
        return fut.result()
    except Exception as e:
        return PuzzleResult(name=Path(path).name, path=path, error=f"future error: {e}")


def _run_batch_mode(
    args: argparse.Namespace,
    state: _RunState,
    solver: RustSolver,
    files: list[str],
    slow_set: set[str],
    total_files: int,
    pool: concurrent.futures.ThreadPoolExecutor,
) -> None:
    slow_timeout = min(args.timeout, 10.0)
    for i in range(0, len(files), args.batch):
        chunk = files[i : i + args.batch]
        # Chunk timeout = max of its members' per-file timeout (slow chunks are
        # rare; a mixed chunk runs at the higher budget).
        ct = max(
            (_file_timeout(f, slow_set, args.timeout, slow_timeout) for f in chunk),
            default=args.timeout,
        )
        for p, r in pool.submit(solve_batch, chunk, ct, solver).result():
            _report(state, args, p, r, total_files)
    pool.shutdown(wait=True)


def _run_parallel_mode(
    args: argparse.Namespace,
    state: _RunState,
    solver: RustSolver,
    files: list[str],
    slow_set: set[str],
    total_files: int,
    pool: concurrent.futures.ThreadPoolExecutor,
    jobs: int,
) -> None:
    slow_timeout = min(args.timeout, 10.0)
    fut_to_path = {
        pool.submit(
            solve_one,
            f,
            _file_timeout(f, slow_set, args.timeout, slow_timeout),
            solver,
        ): f
        for f in files
    }
    for fut in concurrent.futures.as_completed(fut_to_path):
        path = fut_to_path[fut]
        r = _future_result(fut, path)
        _report(state, args, path, r, total_files)
        if args.adaptive_j:
            jobs = _adaptive_throttle(state, jobs, pool)


def _run_executor(
    args: argparse.Namespace,
    state: _RunState,
    solver: RustSolver,
    files: list[str],
    slow_files: list[str],
    total_files: int,
) -> None:
    jobs = args.jobs or os.cpu_count() or 4
    slow_set = set(slow_files) if args.baseline and not args.skip_slow else set()
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        if args.batch > 1:
            _run_batch_mode(args, state, solver, files, slow_set, total_files, pool)
        else:
            _run_parallel_mode(args, state, solver, files, slow_set, total_files, pool, jobs)


def _timeout_failures(state: _RunState) -> list[PuzzleResult]:
    return [
        r
        for r in state.failed
        if r.error and ("timeout" in r.error.lower() or "timed out" in r.error.lower())
    ]


def _handle_retry_result(
    args: argparse.Namespace,
    state: _RunState,
    path: str,
    r: PuzzleResult,
    old_by_path: dict[str, PuzzleResult],
    total_files: int,
) -> bool:
    old = old_by_path.get(path)
    if old is not None:
        _undo(state, args, old)
    state.failed[:] = [fr for fr in state.failed if fr.path != path]
    if not args.summary_only:
        status = "PASS" if (r.solved and r.validated) else "FAIL"
        err = f"  {r.error}" if r.error else ""
        print(
            f"[retry] {status:<4} {_zone(path, args.dir):<8} "
            f"{Path(path).name:<24} via={r.solver or '-':<8} {r.elapsed_ms:>6}ms{err}"
        )
    _report(state, args, path, r, total_files)
    return bool(r.solved and r.validated)


def _run_retry(
    args: argparse.Namespace, state: _RunState, solver: RustSolver, total_files: int
) -> None:
    timeout_failed = _timeout_failures(state)
    if not timeout_failed:
        return
    retry_timeout = args.timeout * 2
    retry_jobs = max(1, (args.jobs or os.cpu_count() or 4) // 4)
    print(
        f"\n⏳ retrying {len(timeout_failed)} timeout puzzles "
        f"(timeout={retry_timeout}s, j={retry_jobs}) ..."
    )
    old_by_path = {r.path: r for r in state.failed}
    retry_passed = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=retry_jobs) as pool2:
        fut_to_path2: dict[concurrent.futures.Future, str] = {}
        for r in timeout_failed:
            fut_to_path2[pool2.submit(solve_one, r.path, retry_timeout, solver)] = r.path
        for fut in concurrent.futures.as_completed(fut_to_path2):
            path = fut_to_path2[fut]
            if _handle_retry_result(
                args, state, path, _future_result(fut, path), old_by_path, total_files
            ):
                retry_passed += 1
    print(f"retry: {retry_passed}/{len(timeout_failed)} recovered")


def _print_run_summary(state: _RunState, skip_names: set[str]) -> None:
    already = len(skip_names)
    grand_total = state.passed + len(state.failed) + already
    grand_passed = state.passed + already  # skipped were already PASS
    print(f"\n结果: {grand_passed}/{grand_total} 通过")
    if already:
        print(f"  (含 {already} 题来自 --resume)")
    for zone in sorted(state.by_zone):
        n, p = state.by_zone[zone]
        print(f"  {zone}: {p} / {n}")


def _finalize(
    state: _RunState,
    baseline: dict[str, dict[str, Any]],
    slow_skipped: list[str],
) -> None:
    # Baseline regression comparison (exit-code precedence: 2 regression >
    # 1 failure > 0 clean).  Checked before the failure exit so a regression
    # is surfaced even when non-regression failures coexist.
    regressions: list[str] = []
    if baseline:
        regressions, _news, _new_files = _print_regression_summary(
            state.current_results, baseline, slow_skipped
        )
    if regressions:
        sys.exit(2)
    if state.failed:
        print("失败:")
        for r in state.failed[:50]:
            print(f"  {r.name}: {r.error}")
        sys.exit(1)
    print("全部验证通过!")


def main() -> None:
    args = _build_arg_parser().parse_args()
    files = _discover_files(args.dir, args.rules)
    if not files:
        print(f"no puzzles found under {args.dir}/")
        sys.exit(1)
    files = _apply_zone_filter(args, files)
    files, skip_names = _apply_resume(args, files)
    baseline = _load_baseline(args.baseline) if args.baseline else {}
    files, slow_files, slow_skipped = _partition_baseline_files(args, files, baseline)
    total_files = len(files)
    if total_files == 0:
        print("all puzzles already passed — nothing to do")
        return

    solver = RustSolver()
    state = _RunState()
    _run_executor(args, state, solver, files, slow_files, total_files)
    if args.retry_timeouts:
        _run_retry(args, state, solver, total_files)
    _print_run_summary(state, skip_names)
    _finalize(state, baseline, slow_skipped)


if __name__ == "__main__":
    main()
