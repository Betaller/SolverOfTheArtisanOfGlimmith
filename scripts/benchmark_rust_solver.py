#!/usr/bin/env python3
"""Benchmark the Rust solver against puzzle corpora.

Features (all optional):
  --resume FILE      skip puzzles already PASS in a previous run
  --adaptive-j       auto-reduce concurrency on OOM (exit -9) or timeout spikes
  --rules RULE       only test puzzles containing this rule type
  --out JSONL        append per-puzzle records to a JSONL file
  --timeout 20       per-puzzle timeout (default 20 s, was 40 s historically)
  -j N / --jobs N    parallel workers (0 = cpu_count)
  --batch N          reuse one rsolver subprocess for every N puzzles

Usage:
  python scripts/benchmark_rust_solver.py
  python scripts/benchmark_rust_solver.py --dir puzzles/official/Zone1 --timeout 20 -j 4
  python scripts/benchmark_rust_solver.py --resume results/prev.txt --adaptive-j
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
        return {"solved": False, "validated": False, "solver": solver,
                "error": out.get("error_message", "no solution")[:300]}
    from types import SimpleNamespace

    regions = []
    for rd in out.get("regions", []):
        cells = [(c[0], c[1]) for c in rd.get("cells", [])]
        regions.append(SimpleNamespace(
            region_id=rd["region_id"], cells=cells,
            area=rd.get("area", len(cells)),
            shape=[(s[0], s[1]) for s in rd.get("shape", [])],
        ))
    sol = SimpleNamespace(board=None, regions=regions, rule_results={})
    board = solution_to_board(puzzle, sol)
    result = IndependentValidator().validate(puzzle, board)
    r: dict[str, Any] = {"solved": result.solved, "validated": result.solved,
                          "solver": solver,
                          "error": "; ".join(result.errors[:3]) if not result.solved else None}
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
        return PuzzleResult(name=name, path=path, elapsed_ms=int(timeout * 1000),
                            error=f"solver error: {e}")

    regions_out = []
    for reg in (solution.regions or []):
        shape_cells = list(reg.shape.cells) if hasattr(reg.shape, 'cells') else (reg.shape or [])
        regions_out.append({"region_id": reg.region_id, "cells": reg.cells,
                            "area": reg.area, "shape": shape_cells})
    r = _validate(puzzle, {
        "solved": solution.solved,
        "solver": solution.solver or "",
        "error_message": solution.error_message,
        "regions": regions_out,
        "elapsed_ms": solution.elapsed_ms,
    }, path)
    return PuzzleResult(name=name, path=path,
                        solved=r["solved"], validated=r["validated"],
                        elapsed_ms=solution.elapsed_ms,
                        error=r.get("error"), solver=r.get("solver", ""),
                        matches_official=r.get("matches_official"))


def solve_batch(paths: list[str], timeout: float, solver: RustSolver) -> list[tuple[str, PuzzleResult]]:
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
            results.append((p, PuzzleResult(name=Path(p).name, path=p,
                                             error=f"batch error: {e}")))
        return results

    for (p, puzzle), sol in zip(pending, solutions):
        regions_out = []
        for reg in (sol.regions or []):
            shape_cells = list(reg.shape.cells) if hasattr(reg.shape, 'cells') else (reg.shape or [])
            regions_out.append({"region_id": reg.region_id, "cells": reg.cells,
                                "area": reg.area, "shape": shape_cells})
        r = _validate(puzzle, {
            "solved": sol.solved,
            "solver": sol.solver or "",
            "error_message": sol.error_message,
            "regions": regions_out,
            "elapsed_ms": sol.elapsed_ms,
        }, p)
        results.append((p, PuzzleResult(name=Path(p).name, path=p,
                                        solved=r["solved"], validated=r["validated"],
                                        elapsed_ms=sol.elapsed_ms,
                                        error=r.get("error"), solver=r.get("solver", ""),
                                        matches_official=r.get("matches_official"))))
    return results


# ── main ───────────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Benchmark Rust solver on puzzles",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""Examples:
  %(prog)s --dir puzzles/official --timeout 20 -j 8
  %(prog)s --resume results/prev.txt --adaptive-j
  %(prog)s --rules block --timeout 30
  %(prog)s --batch 100 -j 1         # batch mode, sequential""",
    )
    parser.add_argument("--dir", default="puzzles/official")
    parser.add_argument("--timeout", type=float, default=20.0, help="per-puzzle timeout in seconds (default 20)")
    parser.add_argument("-j", "--jobs", type=int, default=0, help="parallel workers (0=cpu_count)")
    parser.add_argument("--batch", type=int, default=1, help="batch size for rsolver --batch reuse (default 1)")
    parser.add_argument("--resume", help="skip puzzles already PASS in this file")
    parser.add_argument("--adaptive-j", action="store_true", help="auto-reduce -j on OOM / timeout spikes")
    parser.add_argument("--rules", help="only test puzzles containing this rule type")
    parser.add_argument("--out", help="append JSONL records to this file")
    parser.add_argument("--summary-only", action="store_true", help="only print final summary")
    args = parser.parse_args()

    files = _discover_files(args.dir, args.rules)
    if not files:
        print(f"no puzzles found under {args.dir}/")
        sys.exit(1)

    # Checkpoint resume
    skip_names: set[str] = set()
    if args.resume:
        skip_names = _load_resume_set(args.resume)
        if skip_names:
            before = len(files)
            files = [f for f in files if Path(f).name not in skip_names]
            print(f"resume: skipped {before - len(files)} already-passed, {len(files)} remaining")

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
    oom_streak = 0
    timeout_streak = 0
    max_oom_streak = 3
    max_timeout_streak = 5

    def report(path: str, r: PuzzleResult) -> None:
        nonlocal passed, oom_streak, timeout_streak
        zone = _zone(path, args.dir)
        zt, zp = by_zone.get(zone, (0, 0))
        by_zone[zone] = (zt + 1, zp + (1 if r.solved else 0))

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
            print(f"[{passed + len(failed):>4}/{total_files}] {status:<4} {zone:<8} "
                  f"{r.name:<24} via={r.solver or '-':<8} {r.elapsed_ms:>6}ms"
                  f"{'  ' + r.error if r.error else ''}")

        if args.out:
            with open(args.out, "a", encoding="utf-8") as fh:
                fh.write(json.dumps({
                    "file": path, "name": r.name, "zone": zone,
                    "status": status, "solved": r.solved, "validated": r.validated,
                    "elapsed_ms": r.elapsed_ms, "solver": r.solver, "error": r.error,
                    "matches_official": r.matches_official,
                }, ensure_ascii=False) + "\n")

    # ── executor with adaptive-j ──────────────────────────────────────────
    processed = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        done = 0

        def _on_done(fut: concurrent.futures.Future) -> None:
            nonlocal done, oom_streak, timeout_streak, jobs
            done += 1

        if args.batch > 1:
            chunks = [files[i:i + args.batch] for i in range(0, len(files), args.batch)]
            for chunk in chunks:
                fut = pool.submit(solve_batch, chunk, args.timeout, solver)
                fut.add_done_callback(_on_done)
                for p, r in fut.result():
                    report(p, r)

            # Wait for remaining
            pool.shutdown(wait=True)
        else:
            fut_to_path: dict[concurrent.futures.Future, str] = {}
            for f in files:
                fut = pool.submit(solve_one, f, args.timeout, solver)
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
                        print(f"⚠ timeout streak {timeout_streak}, reducing concurrency to j={jobs}")
                        timeout_streak = 0

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
    if failed:
        print("失败:")
        for r in failed[:50]:
            print(f"  {r.name}: {r.error}")
        sys.exit(1)
    print("全部验证通过!")


if __name__ == "__main__":
    main()
