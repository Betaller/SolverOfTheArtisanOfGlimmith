"""Benchmark the Rust solver against all official puzzles.

Mirrors the C++ solver benchmark (third_party/AoG_Solver): each puzzle gets a
20s timeout; a puzzle counts as solved when the Rust solver returns a solution
that passes the independent validator.

Usage:
    python scripts/benchmark_rust_solver.py            # default dir: puzzles/official
    python scripts/benchmark_rust_solver.py --dir puzzles/official/Zone2 -j 8
"""
from __future__ import annotations

import argparse
import concurrent.futures
import glob
import json
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, ".")

from src.io.puzzle_codec import dict_to_puzzle
from src.validation.official_answer import matches_official_answer
from src.validation.validator import IndependentValidator, solution_to_board

ROOT = Path(__file__).resolve().parent.parent
BIN = None


def _find_binary() -> Path:
    global BIN
    if BIN is not None:
        return BIN
    for profile in ("release", "debug"):
        p = ROOT / "rsolver" / "target" / profile / ("rsolver.exe" if sys.platform == "win32" else "rsolver")
        if p.is_file():
            BIN = p
            return p
    raise FileNotFoundError("rsolver binary not built. Run: cd rsolver && cargo build --release")


def _solve_one_line(out: dict, puzzle, ms: int, path: str | None = None) -> dict:
    """Validate one solved puzzle against the independent validator, and (when
    ``path`` names an official puzzle with an answer file) compare the region
    partition against the official unique solution."""
    from types import SimpleNamespace
    regions = []
    for rd in out.get("regions", []):
        cells = [(c[0], c[1]) for c in rd.get("cells", [])]
        shape_cells = [(s[0], s[1]) for s in rd.get("shape", [])]
        regions.append(SimpleNamespace(
            region_id=rd["region_id"], cells=cells,
            area=rd.get("area", len(cells)), shape=shape_cells,
        ))
    sol = SimpleNamespace(board=None, regions=regions, rule_results={})
    board = solution_to_board(puzzle, sol)
    result = IndependentValidator().validate(puzzle, board)
    r = {"solved": result.solved, "validated": result.solved, "ms": ms,
         "error": "; ".join(result.errors[:3]) if not result.solved else None}
    if result.solved and path is not None:
        r["matches_official"] = matches_official_answer(path, [reg.cells for reg in regions])
    return r


def test_batch(paths: list[str], timeout: float) -> list[tuple[str, dict]]:
    """Solve a chunk of puzzles in ONE `rsolver --batch` subprocess
    (line-delimited JSON in/out), reusing the process instead of spawning one
    per puzzle.  Returns `(path, result_dict)` pairs preserving input order.
    """
    pending: list[tuple[str, str, object, dict]] = []  # (path, name, puzzle, data)
    results: list[tuple[str, dict]] = []
    for path in paths:
        name = Path(path).name
        try:
            with open(path, encoding="utf-8") as f:
                data = json.load(f)
            puzzle = dict_to_puzzle(data)
        except Exception as e:
            results.append((path, {"name": name, "solved": False, "validated": False,
                                   "ms": 0, "error": f"load error: {e}"}))
            continue
        if not puzzle.rules:
            results.append((path, {"name": name, "solved": True, "validated": True,
                                   "ms": 0, "error": None}))
            continue
        pending.append((path, name, puzzle, data))

    if pending:
        input_data = "\n".join(
            json.dumps(data, ensure_ascii=True)
            for _, _, _, data in pending
        ) + "\n"
        start = time.monotonic()
        timeout_msg = f"timeout ({len(pending)}×{timeout:g}s)"
        try:
            proc = subprocess.run(
                [str(_find_binary()), "--batch"], input=input_data,
                capture_output=True, text=True, timeout=len(pending) * timeout,
                encoding="utf-8",
            )
        except subprocess.TimeoutExpired as te:
            # Preserve puzzles that already emitted a full line; only the
            # runaway and the puzzles queued behind it are marked failed.
            partial = te.stdout or ""
            if isinstance(partial, bytes):
                partial = partial.decode("utf-8", errors="replace")
            # Keep only complete (newline-terminated) lines.
            parts = partial.split("\n")
            if not partial.endswith("\n"):
                parts = parts[:-1]
            outs = [l for l in parts if l.strip()]
            for i, (path, name, puzzle, _) in enumerate(pending):
                if i < len(outs):
                    try:
                        out = json.loads(outs[i])
                    except json.JSONDecodeError:
                        out = {"solved": False}
                    if out.get("solved"):
                        r = _solve_one_line(out, puzzle, out.get("elapsed_ms", 0), path)
                        results.append((path, {"name": name, **r}))
                        continue
                results.append((path, {"name": name, "solved": False, "validated": False,
                                       "ms": int(timeout * 1000), "error": timeout_msg}))
            return results
        wall_ms = int((time.monotonic() - start) * 1000)

        if proc.returncode != 0:
            for path, name, _, _ in pending:
                results.append((path, {"name": name, "solved": False, "validated": False,
                                       "ms": wall_ms,
                                       "error": f"exit {proc.returncode}: {proc.stderr[:300]}"}))
            return results

        outs = [l for l in proc.stdout.splitlines() if l.strip()]
        for i, (path, name, puzzle, _) in enumerate(pending):
            if i >= len(outs):
                results.append((path, {"name": name, "solved": False, "validated": False,
                                       "ms": wall_ms, "error": "batch truncated"}))
                continue
            try:
                out = json.loads(outs[i])
            except json.JSONDecodeError as e:
                results.append((path, {"name": name, "solved": False, "validated": False,
                                       "ms": wall_ms, "error": f"bad json: {e}"}))
                continue
            if not out.get("solved"):
                results.append((path, {"name": name, "solved": False, "validated": False,
                                       "ms": wall_ms,
                                       "error": out.get("error_message", "no solution")[:300]}))
                continue
            r = _solve_one_line(out, puzzle, out.get("elapsed_ms", wall_ms), path)
            r = {"name": name, **r}
            results.append((path, r))
    return results


def test_one(path: str, timeout: float) -> dict:
    name = Path(path).name
    try:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
        puzzle = dict_to_puzzle(data)
    except Exception as e:
        return {"name": name, "solved": False, "validated": False, "ms": 0,
                "error": f"load error: {e}"}
    if not puzzle.rules:
        return {"name": name, "solved": True, "validated": True, "ms": 0, "error": None}

    input_json = json.dumps(data, ensure_ascii=True)
    start = time.monotonic()
    try:
        proc = subprocess.run(
            [str(_find_binary())], input=input_json,
            capture_output=True, text=True, timeout=timeout,
            encoding="utf-8",
        )
    except subprocess.TimeoutExpired:
        return {"name": name, "solved": False, "validated": False, "ms": int(timeout * 1000),
                "error": "timeout"}
    ms = int((time.monotonic() - start) * 1000)

    if proc.returncode != 0:
        return {"name": name, "solved": False, "validated": False, "ms": ms,
                "error": f"exit {proc.returncode}: {proc.stderr[:300]}"}
    try:
        out = json.loads(proc.stdout)
    except json.JSONDecodeError as e:
        return {"name": name, "solved": False, "validated": False, "ms": ms, "error": f"bad json: {e}"}

    if not out.get("solved"):
        return {"name": name, "solved": False, "validated": False, "ms": ms,
                "error": out.get("error_message", "no solution")[:300]}

    from types import SimpleNamespace
    regions = []
    for rd in out.get("regions", []):
        cells = [(c[0], c[1]) for c in rd.get("cells", [])]
        shape_cells = [(s[0], s[1]) for s in rd.get("shape", [])]
        regions.append(SimpleNamespace(
            region_id=rd["region_id"], cells=cells,
            area=rd.get("area", len(cells)), shape=shape_cells,
        ))

    sol = SimpleNamespace(board=None, regions=regions, rule_results={})
    board = solution_to_board(puzzle, sol)
    result = IndependentValidator().validate(puzzle, board)
    r = {"name": name, "solved": result.solved, "validated": result.solved, "ms": ms,
         "error": "; ".join(result.errors[:3]) if not result.solved else None}
    if result.solved:
        r["matches_official"] = matches_official_answer(path, [reg.cells for reg in regions])
    return r


def main() -> None:
    parser = argparse.ArgumentParser(description="Benchmark Rust solver on official puzzles")
    parser.add_argument("--dir", default="puzzles/official", help="puzzle directory")
    parser.add_argument("--timeout", type=float, default=20.0, help="per-puzzle timeout (s)")
    parser.add_argument("-j", "--jobs", type=int, default=0, help="parallel workers (0 = CPU cores)")
    parser.add_argument("--batch", type=int, default=1,
                        help="每批谜题数，复用同一个 rsolver 子进程（默认 1=逐题 spawn）")
    args = parser.parse_args()

    # Skip metadata/index files (e.g. `_index.json`) and official-answer dirs
    # (`*-answer`) — they describe the puzzle set / official answers, not
    # solvable puzzles.
    files = sorted(
        f for f in glob.glob(f"{args.dir}/**/*.json", recursive=True)
        if not Path(f).name.startswith("_")
        and not any(part.endswith("-answer") for part in Path(f).parts)
    )
    if not files:
        print(f"no puzzles found under {args.dir}/")
        sys.exit(1)

    total = passed = 0
    failed: list[dict] = []
    by_zone: dict[str, tuple[int, int]] = {}

    def report(path: str, r: dict) -> None:
        nonlocal total, passed
        total += 1
        rel = Path(path).relative_to(args.dir).parts
        zone = rel[0] if len(rel) > 1 else "?"
        z = by_zone.get(zone, (0, 0))
        by_zone[zone] = (z[0] + 1, z[1] + (1 if r["solved"] else 0))
        # solved & validated but partition ≠ official unique solution → DIFF
        diff = r.get("solved") and r.get("matches_official") is False
        status = "DIFF" if diff else ("PASS" if r.get("solved") else "FAIL")
        if diff:
            r["error"] = (r.get("error") or "") + " 解与官方题解不一致"
            failed.append(r)
        elif r.get("solved"):
            passed += 1
        else:
            failed.append(r)
        print(f"[{total:>3}/{total}] {status} {zone:<6} {r['name']:<22} {r['ms']:>6}ms"
              f"{'  ' + str(r['error']) if r.get('error') else ''}")

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs or None) as pool:
        done = 0
        if args.batch > 1:
            chunks = [files[i:i + args.batch] for i in range(0, len(files), args.batch)]
            fut_map = {pool.submit(test_batch, chunk, args.timeout): chunk for chunk in chunks}
            for fut in concurrent.futures.as_completed(fut_map):
                done += 1
                for path, r in fut.result():
                    report(path, r)
        else:
            fut_map = {pool.submit(test_one, f, args.timeout): f for f in files}
            for fut in concurrent.futures.as_completed(fut_map):
                done += 1
                report(fut_map[fut], fut.result())

    print(f"\n结果: {passed}/{total} 通过")
    for zone in sorted(by_zone):
        n, p = by_zone[zone]
        print(f"  {zone}: {p} / {n}")
    if failed:
        print("失败:")
        for r in failed[:40]:
            print(f"  {r['name']}: {r['error']}")
        sys.exit(1)


if __name__ == "__main__":
    main()
