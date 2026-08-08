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
import subprocess
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


def _load_resume_set(path: str) -> set[str]:
    """Read a previous benchmark output and return the set of *passed* puzzle names."""
    passed: set[str] = set()
    if not os.path.isfile(path):
        return passed
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            # Lines look like:  [N/M] PASS Zone1   name.json  ...
            if line.startswith("[") and "\tPASS " in line.replace("  ", "\t"):
                try:
                    name = line.split()[-3]  # "name.json"
                    if name.endswith(".json"):
                        passed.add(name)
                except (IndexError, ValueError):
                    continue
            elif line.startswith("[") and " PASS " in line:
                try:
                    parts = line.split()
                    for p in parts:
                        if p.endswith(".json"):
                            passed.add(p)
                            break
                except (IndexError, ValueError):
                    continue
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
        prev = baseline.get(key)
        if prev is None:
            new_files.append(key)
            continue
        prev_pass = prev.get("status") == "PASS"
        now_pass = r.solved and r.validated
        if prev_pass and not now_pass:
            regressions.append(key)
        elif not prev_pass and now_pass:
            news.append(key)

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

    for (p, puzzle), sol in zip(pending, solutions):
        regions_out = []
        for reg in sol.regions or []:
            shape_cells = (
                list(reg.shape.cells) if hasattr(reg.shape, "cells") else (reg.shape or [])
            )
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
            p,
        )
        results.append(
            (
                p,
                PuzzleResult(
                    name=Path(p).name,
                    path=p,
                    solved=r["solved"],
                    validated=r["validated"],
                    elapsed_ms=sol.elapsed_ms,
                    error=r.get("error"),
                    solver=r.get("solver", ""),
                    matches_official=r.get("matches_official"),
                ),
            )
        )
    return results


# ── main ───────────────────────────────────────────────────────────────────


def main() -> None:
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
    args = parser.parse_args()

    files = _discover_files(args.dir, args.rules)
    if not files:
        print(f"no puzzles found under {args.dir}/")
        sys.exit(1)

    # Zone filter (composes with --rules).
    if args.zone:
        files = [f for f in files if _zone(f, args.dir) == args.zone]
        if not files:
            print(f"no puzzles in zone {args.zone} under {args.dir}/")
            sys.exit(1)

    # Checkpoint resume
    skip_names: set[str] = set()
    if args.resume:
        skip_names = _load_resume_set(args.resume)
        if skip_names:
            before = len(files)
            files = [f for f in files if Path(f).name not in skip_names]
            print(f"resume: skipped {before - len(files)} already-passed, {len(files)} remaining")

    # ── baseline regression mode: partition into main / known-slow buckets ──
    # Iterate over discovered files (not baseline records) so puzzles absent
    # from the baseline (new corpus additions / different --dir) are still run.
    baseline: dict[str, dict[str, Any]] = {}
    slow_files: list[str] = []  # known-slow baseline-FAILs, probed last
    slow_skipped: list[str] = []  # keys skipped via --skip-slow (for summary)
    if args.baseline:
        baseline = _load_baseline(args.baseline)
        if not baseline:
            print(f"warning: --baseline {args.baseline} empty/unreadable; running full.")
        # Auto-default --out so a fresh baseline is always produced.
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
            files = main_files
        else:
            print(f"baseline: {n_main} main + {n_slow} known-slow (probed last @ min(timeout,10)s)")
            files = main_files + slow_files  # slow last, but run at reduced timeout

    total_files = len(files)
    if total_files == 0:
        print("all puzzles already passed — nothing to do")
        return

    solver = RustSolver()
    passed = 0
    failed: list[PuzzleResult] = []
    by_zone: dict[str, tuple[int, int]] = {}  # zone -> (total, passed)

    # Adaptive concurrency
    jobs = args.jobs or os.cpu_count() or 4
    original_jobs = jobs  # before adaptive-j mutates `jobs` (for --retry-timeouts)
    oom_streak = 0
    timeout_streak = 0
    max_oom_streak = 3
    max_timeout_streak = 5

    # Per-file timeout: known-slow baseline-FAILs (non --skip-slow) are probed
    # at a reduced timeout so they don't dominate the quick-tier wall-clock.
    slow_set = set(slow_files) if args.baseline and not args.skip_slow else set()
    slow_timeout = min(args.timeout, 10.0)

    def file_timeout(path: str) -> float:
        return slow_timeout if path in slow_set else args.timeout

    # current_results keyed by baseline_key — populated by report() for the
    # regression summary (--baseline mode).
    current_results: dict[str, PuzzleResult] = {}

    def report(path: str, r: PuzzleResult) -> None:
        nonlocal passed, oom_streak, timeout_streak
        zone = _zone(path, args.dir)
        zt, zp = by_zone.get(zone, (0, 0))
        by_zone[zone] = (zt + 1, zp + (1 if r.solved else 0))
        current_results[_baseline_key(zone, r.name)] = r

        diff = r.solved and r.validated and r.matches_official is False
        status = "DIFF" if diff else ("PASS" if r.solved and r.validated else "FAIL")
        if diff:
            r.error = (r.error or "") + " 解与官方题解不一致"
            failed.append(r)
        elif r.solved and r.validated:
            passed += 1
        else:
            failed.append(r)

        # Adaptive concurrency tracking
        err = r.error or ""
        if "exit -9" in err:
            oom_streak += 1
            timeout_streak = 0
        elif "timeout" in err.lower():
            timeout_streak += 1
            oom_streak = 0
        else:
            oom_streak = 0
            timeout_streak = 0

        if not args.summary_only:
            print(
                f"[{passed + len(failed):>4}/{total_files}] {status:<4} {zone:<8} "
                f"{r.name:<24} via={r.solver or '-':<8} {r.elapsed_ms:>6}ms"
                f"{'  ' + r.error if r.error else ''}"
            )

        if args.out:
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
                        },
                        ensure_ascii=False,
                    )
                    + "\n"
                )

    # ── executor with adaptive-j ──────────────────────────────────────────
    processed = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        done = 0

        def _on_done(fut: concurrent.futures.Future) -> None:
            nonlocal done, oom_streak, timeout_streak, jobs
            done += 1

        if args.batch > 1:
            chunks = [files[i : i + args.batch] for i in range(0, len(files), args.batch)]
            for chunk in chunks:
                # Chunk timeout = max of its members' per-file timeout (slow
                # chunks are rare; a mixed chunk runs at the higher budget).
                ct = max((file_timeout(f) for f in chunk), default=args.timeout)
                fut = pool.submit(solve_batch, chunk, ct, solver)
                fut.add_done_callback(_on_done)
                for p, r in fut.result():
                    report(p, r)

            # Wait for remaining
            pool.shutdown(wait=True)
        else:
            fut_to_path: dict[concurrent.futures.Future, str] = {}
            for f in files:
                fut = pool.submit(solve_one, f, file_timeout(f), solver)
                fut_to_path[fut] = f

            # Adaptive-j: watch for OOM/timeout streaks and throttle
            for fut in concurrent.futures.as_completed(fut_to_path):
                path = fut_to_path[fut]
                try:
                    r = fut.result()
                except Exception as e:
                    r = PuzzleResult(name=Path(path).name, path=path, error=f"future error: {e}")
                report(path, r)
                processed += 1

                if args.adaptive_j:
                    if oom_streak >= max_oom_streak and jobs > 1:
                        jobs = max(1, jobs // 2)
                        print(f"⚠ OOM streak {oom_streak}, reducing concurrency to j={jobs}")
                        oom_streak = 0
                        # Restart pool with fewer workers
                        pool._max_workers = jobs
                    elif timeout_streak >= max_timeout_streak and jobs > 2:
                        jobs = max(1, jobs - 2)
                        print(
                            f"⚠ timeout streak {timeout_streak}, reducing concurrency to j={jobs}"
                        )
                        timeout_streak = 0

    # ── retry timeouts ─────────────────────────────────────────────────────
    # Three old bugs fixed: (1) retry results now go through report() so the
    # corrected record is appended to --out JSONL (last-wins in _load_baseline,
    # covering the stale original); (2) retry_jobs uses original_jobs, not the
    # adaptive-j-mutated `jobs`; (3) retry-FAIL also re-reports so stale
    # elapsed_ms/error don't linger.  Functional only after the Rust timeout
    # passthrough fix (retry_timeout now actually reaches the Rust search).
    if args.retry_timeouts:
        timeout_failed = [
            r
            for r in failed
            if r.error and ("timeout" in r.error.lower() or "timed out" in r.error.lower())
        ]
        if timeout_failed:
            retry_timeout = args.timeout * 2
            retry_jobs = max(1, original_jobs // 4)  # 8→2, 4→1
            print(
                f"\n⏳ retrying {len(timeout_failed)} timeout puzzles "
                f"(timeout={retry_timeout}s, j={retry_jobs}) ..."
            )
            retry_paths = [r.path for r in timeout_failed]
            # Index the original results by path so we can undo their accounting
            # before re-reporting (report() re-adds passed/failed/by_zone and
            # appends a fresh JSONL line that supersedes the stale one).
            old_by_path = {r.path: r for r in failed}
            retry_passed = 0

            def _undo(old_r: PuzzleResult) -> None:
                nonlocal passed
                zone = _zone(old_r.path, args.dir)
                zt, zp = by_zone.get(zone, (0, 0))
                # Reverse the +1 total / +1 passed-or-not applied by report().
                by_zone[zone] = (zt - 1, zp - (1 if old_r.solved and old_r.validated else 0))
                if old_r.solved and old_r.validated:
                    passed -= 1

            with concurrent.futures.ThreadPoolExecutor(max_workers=retry_jobs) as pool2:
                fut_to_path2: dict[concurrent.futures.Future, str] = {}
                for f in retry_paths:
                    fut = pool2.submit(solve_one, f, retry_timeout, solver)
                    fut_to_path2[fut] = f
                for fut in concurrent.futures.as_completed(fut_to_path2):
                    path = fut_to_path2[fut]
                    try:
                        r = fut.result()
                    except Exception as e:
                        r = PuzzleResult(
                            name=Path(path).name, path=path, error=f"future error: {e}"
                        )
                    if r.solved and r.validated:
                        retry_passed += 1
                    # Undo the old record, drop it from `failed`, then re-report
                    # (appends corrected JSONL line; updates by_zone/passed).
                    old = old_by_path.get(path)
                    if old is not None:
                        _undo(old)
                    failed[:] = [fr for fr in failed if fr.path != path]
                    if not args.summary_only:
                        status = "PASS" if (r.solved and r.validated) else "FAIL"
                        print(
                            f"[retry] {status:<4} {_zone(path, args.dir):<8} "
                            f"{Path(path).name:<24} via={r.solver or '-':<8} {r.elapsed_ms:>6}ms"
                            f"{'  ' + (r.error or '') if r.error else ''}"
                        )
                    report(path, r)
            print(f"retry: {retry_passed}/{len(timeout_failed)} recovered")

    # ── summary ────────────────────────────────────────────────────────────
    total_in_run = passed + len(failed)
    already = len(skip_names)
    grand_total = total_in_run + already
    grand_passed = passed + already  # skipped were already PASS
    print(f"\n结果: {grand_passed}/{grand_total} 通过")
    if already:
        print(f"  (含 {already} 题来自 --resume)")
    for zone in sorted(by_zone):
        n, p = by_zone[zone]
        print(f"  {zone}: {p} / {n}")

    # Baseline regression comparison (exit-code precedence: 2 regression >
    # 1 failure > 0 clean).  Checked before the failure exit so a regression
    # is surfaced even when non-regression failures coexist.
    regressions: list[str] = []
    if args.baseline and baseline:
        regressions, _news, _new_files = _print_regression_summary(
            current_results,
            baseline,
            slow_skipped,
        )

    if regressions:
        sys.exit(2)
    if failed:
        print("失败:")
        for r in failed[:50]:
            print(f"  {r.name}: {r.error}")
        sys.exit(1)
    print("全部验证通过!")


if __name__ == "__main__":
    main()
