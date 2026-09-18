#!/usr/bin/env python3
"""Solo rsolver evaluation: run the release binary on a list of puzzles, one at a
time (no parallel load), with a fixed RSOLVER_TIMEOUT_MS budget.  Isolates
solver-ordering / pruning effects from benchmark parallel-load noise.

Writes one JSON result line per puzzle immediately (append + flush) so progress
survives interruptions.  Pass --resume <jsonl> to skip puzzles already recorded.

Usage: python3 scripts/solo_eval.py <puzzle_list.txt> <timeout_seconds>
       <out_jsonl> [--resume <jsonl>]
"""

import contextlib
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "rsolver", "target", "release", "rsolver")


def run_one(path: str, timeout_ms: int, wall_cap: float) -> dict:
    try:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
    except Exception as e:
        return {"name": os.path.basename(path), "solved": False, "error": f"load: {e}"}
    env = {**os.environ, "RSOLVER_TIMEOUT_MS": str(timeout_ms)}
    t0 = time.time()
    try:
        proc = subprocess.run(
            [BIN],
            input=json.dumps(data),
            capture_output=True,
            text=True,
            timeout=wall_cap,
            env=env,
        )
    except subprocess.TimeoutExpired:
        return {
            "name": os.path.basename(path),
            "solved": False,
            "error": "wall-timeout",
            "elapsed_ms": int((time.time() - t0) * 1000),
        }
    elapsed = int((time.time() - t0) * 1000)
    out = proc.stdout.strip()
    if not out:
        return {
            "name": os.path.basename(path),
            "solved": False,
            "error": "no-output",
            "stderr": proc.stderr[:200],
            "elapsed_ms": elapsed,
        }
    try:
        sol = json.loads(out.splitlines()[-1])
    except Exception as e:
        return {
            "name": os.path.basename(path),
            "solved": False,
            "error": f"parse: {e}",
            "elapsed_ms": elapsed,
        }
    return {
        "name": os.path.basename(path),
        "solved": bool(sol.get("solved")),
        "solver": sol.get("solver", ""),
        "elapsed_ms": elapsed,
    }


def parse_args(argv: list[str]) -> tuple[str, float, str | None, str | None] | None:
    """(pathfile, timeout_s, out, resume) or None when argv is too short."""
    if len(argv) < 3:
        return None
    out = argv[3] if len(argv) > 3 else None
    resume = argv[argv.index("--resume") + 1] if "--resume" in argv else None
    return argv[1], float(argv[2]), out, resume


def load_done(resume: str | None) -> set:
    """Names already recorded in the resume jsonl (empty set when unusable)."""
    done = set()
    if not (resume and os.path.isfile(resume)):
        return done
    with open(resume) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            with contextlib.suppress(Exception):
                done.add(json.loads(line)["name"])
    return done


def emit(r: dict, fh) -> bool:
    """Append + print one result line; returns whether it was solved."""
    if fh:
        fh.write(json.dumps(r) + "\n")
        fh.flush()
    print(
        f"{'PASS' if r['solved'] else 'FAIL'} {r['name']:20s} "
        f"{r.get('elapsed_ms', 0):>7}ms {r.get('solver', '')}",
        flush=True,
    )
    return bool(r["solved"])


def _solo_run(paths: list[str], done: set, timeout_s: float, wall_cap: float, fh) -> int:
    """Iterate puzzles, emit results, return the count solved this run."""
    nsolved = 0
    for p in paths:
        name = os.path.basename(p)
        if name in done:
            continue
        if emit(run_one(p, int(timeout_s * 1000), wall_cap), fh):
            nsolved += 1
    return nsolved


def main() -> int:
    parsed = parse_args(sys.argv)
    if parsed is None:
        print(__doc__)
        return 2
    pathfile, timeout_s, out, resume = parsed
    with open(pathfile) as pf:
        paths = [p.strip() for p in pf if p.strip()]
    done = load_done(resume)
    # router runs modules sequentially: aog(20)+rose(12)+edge_csp(40) ~= 72s
    wall_cap = timeout_s * 2.3
    if out:
        with open(out, "a") as fh:
            nsolved = _solo_run(paths, done, timeout_s, wall_cap, fh)
    else:
        nsolved = _solo_run(paths, done, timeout_s, wall_cap, None)
    print(f"\nSOLVED {nsolved}/{len(paths)} (this run; resume-aware)", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
