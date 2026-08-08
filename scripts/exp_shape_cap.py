#!/usr/bin/env python3
"""Experiment: run the 21 OOM puzzles with three AOG_SHAPE_CAP values.

Each puzzle is fed directly to the rsolver binary via stdin (one process per
puzzle, no parallelism — clean solo timing). Records solved / exit code /
elapsed_ms for each (cap, puzzle) pair.

Usage:
  AOG_SHAPE_CAP env not used here — we set it per-run.
  Run from repo root: .venv/bin/python scripts/exp_shape_cap.py
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

BIN = Path("rsolver/target/release/rsolver")
TIMEOUT_MS = 40_000  # per-puzzle deadline; SIGKILL at +50% as safety

# 21 OOM puzzles (exit -9 on baseline bd2f5f5), with their subdirs.
OOM = [
    ("Zone1/11-mixed-rules", "0882"),
    ("Zone2/8-all-rectangles", "0826"),
    ("Zone2/9-no-rectangles", "0838"),
    ("Zone3/1-tatami", "0957"),
    ("Zone3/2-loopy", "0606"),
    ("Zone3/2-loopy", "0690"),
    ("Zone3/2-loopy", "0834"),
    ("Zone3/2-loopy", "0969"),
    ("Zone3/2-loopy", "0976"),
    ("Zone3/2-loopy", "0977"),
    ("Zone3/2-loopy", "0978"),
    ("Zone3/2-loopy", "0980"),
    ("Zone3/2-loopy", "1091"),
    ("Zone3/2-loopy", "1215"),
    ("Zone3/2-loopy", "1373"),
    ("Zone3/2-loopy", "1375"),
    ("Zone3/2-loopy", "1378"),
    ("Zone3/3-vertex-radar", "0491"),
    ("Zone3/3-vertex-radar", "0999"),
    ("Zone3/7-zone3-mixed", "0629"),
    ("Zone3/7-zone3-mixed", "1110"),
]

CAPS = [50000, 100000, 200000]


def run_one(puzzle_path: Path, cap: int) -> dict:
    env = os.environ.copy()
    env["RSOLVER_TIMEOUT_MS"] = str(TIMEOUT_MS)
    env["AOG_SHAPE_CAP"] = str(cap)
    data = puzzle_path.read_bytes()
    t0 = time.monotonic()
    try:
        proc = subprocess.run(
            [str(BIN)],
            input=data,
            capture_output=True,
            timeout=int(TIMEOUT_MS * 2 / 1000) + 30,  # hard kill safety
            env=env,
        )
        wall_ms = int((time.monotonic() - t0) * 1000)
        out = proc.stdout.decode(errors="replace").strip()
        err = proc.stderr.decode(errors="replace").strip()
        exit_code = proc.returncode
        solved = None
        elapsed_ms = None
        err_msg = ""
        if out:
            try:
                rec = json.loads(out.splitlines()[-1])
                solved = rec.get("solved")
                elapsed_ms = rec.get("elapsed_ms")
                err_msg = rec.get("error_message", "") or ""
            except Exception as e:
                err_msg = f"parse: {e}; out={out[:200]}"
        return {
            "exit": exit_code,
            "solved": solved,
            "elapsed_ms": elapsed_ms,
            "wall_ms": wall_ms,
            "err": err_msg[:120],
            "stderr_tail": err[-200:] if err else "",
        }
    except subprocess.TimeoutExpired:
        return {
            "exit": "KILLED",
            "solved": None,
            "elapsed_ms": None,
            "wall_ms": int((time.monotonic() - t0) * 1000),
            "err": "subprocess timeout (hard kill)",
            "stderr_tail": "",
        }


def main() -> None:
    if not BIN.exists():
        sys.exit(f"missing {BIN}; build first: cd rsolver && cargo build --release")
    base = Path("puzzles/official")
    results = {}
    for cap in CAPS:
        print(f"\n=== AOG_SHAPE_CAP={cap} ===", flush=True)
        cap_results = {}
        for zone_sub, name in OOM:
            p = base / zone_sub / f"{name}.json"
            if not p.exists():
                print(f"  {name}: MISSING {p}", flush=True)
                continue
            r = run_one(p, cap)
            cap_results[name] = r
            tag = "PASS" if r["solved"] else ("OOM" if r["exit"] == -9 else "fail")
            print(
                f"  {name:<6} exit={str(r['exit']):>5} solved={r['solved']} "
                f"elapsed={r['elapsed_ms']}ms wall={r['wall_ms']}ms [{tag}] {r['err'][:60]}",
                flush=True,
            )
        results[str(cap)] = cap_results
        solved_n = sum(1 for r in cap_results.values() if r["solved"])
        oom_n = sum(1 for r in cap_results.values() if r["exit"] == -9)
        panic_n = sum(1 for r in cap_results.values() if r["exit"] == 101)
        print(
            f"  --- cap={cap}: solved={solved_n}/21  OOM={oom_n}  panic={panic_n}",
            flush=True,
        )

    out_path = Path("results/tmp/20260808_shapecap_experiment.json")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(results, indent=2))
    print(f"\nwritten: {out_path}", flush=True)


if __name__ == "__main__":
    main()
