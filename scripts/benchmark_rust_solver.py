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
    return {"name": name, "solved": result.solved, "validated": result.solved, "ms": ms,
            "error": "; ".join(result.errors[:3]) if not result.solved else None}


def main() -> None:
    parser = argparse.ArgumentParser(description="Benchmark Rust solver on official puzzles")
    parser.add_argument("--dir", default="puzzles/official", help="puzzle directory")
    parser.add_argument("--timeout", type=float, default=20.0, help="per-puzzle timeout (s)")
    parser.add_argument("-j", "--jobs", type=int, default=0, help="parallel workers (0 = CPU cores)")
    args = parser.parse_args()

    # Skip metadata/index files (e.g. `_index.json`) — they describe the puzzle
    # set but are not themselves solvable puzzles.
    files = sorted(
        f for f in glob.glob(f"{args.dir}/**/*.json", recursive=True)
        if not Path(f).name.startswith("_")
    )
    if not files:
        print(f"no puzzles found under {args.dir}/")
        sys.exit(1)

    total = passed = 0
    failed: list[dict] = []
    by_zone: dict[str, tuple[int, int]] = {}

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs or None) as pool:
        fut_map = {pool.submit(test_one, f, args.timeout): f for f in files}
        done = 0
        for fut in concurrent.futures.as_completed(fut_map):
            done += 1
            r = fut.result()
            total += 1
            rel = Path(fut_map[fut]).relative_to(args.dir).parts
            zone = rel[0] if len(rel) > 1 else "?"
            z = by_zone.get(zone, (0, 0))
            by_zone[zone] = (z[0] + 1, z[1] + (1 if r["solved"] else 0))
            if r["solved"]:
                passed += 1
            else:
                failed.append(r)
            print(f"[{done:>3}/{total}] {'PASS' if r['solved'] else 'FAIL'} "
                  f"{zone:<6} {r['name']:<22} {r['ms']:>6}ms"
                  f"{'  ' + str(r['error']) if r['error'] else ''}")

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
