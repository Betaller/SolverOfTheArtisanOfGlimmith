#!/usr/bin/env python3
"""A1 K-bounding experiment: test K-locked FAIL puzzles (precise/solitary/rose)."""
import json, pathlib, subprocess, os, sys
BIN = "rsolver/target/release/rsolver"
bench = {}
for line in open("results/bench/latest.jsonl"):
    r = json.loads(line.strip()); bench[(r.get("zone",""), r.get("name",""))] = r
tested = new_pass = still_fail = panic = 0
for f in pathlib.Path("puzzles/official").rglob("*.json"):
    if "answer" in str(f): continue
    try: p = json.load(open(f))
    except: continue
    rules = set(r.get("type") for r in p.get("rules",[]))
    g = p.get("grid",{}); h,w = g.get("height",0), g.get("width",0)
    cells = p.get("cells",[]); blocked = sum(1 for c in cells if c.get("blocked"))
    fill = h*w - blocked
    is_k = False
    if "precise" in rules:
        ns = [r.get("params",{}).get("area") for r in p.get("rules",[]) if r.get("type")=="precise"]
        ns = [n for n in ns if n]
        if ns and fill % ns[0] == 0: is_k = True
    elif "solitary" in rules:
        clue = sum(1 for c in cells if c.get("compass") or c.get("symbol") or c.get("shape_pattern") or c.get("fence_pattern"))
        if clue > 0: is_k = True
    elif "rose_window" in rules:
        syms = [c.get("symbol") for c in cells if c.get("symbol")]
        if syms: is_k = True
    if not is_k: continue
    zone = str(f.relative_to("puzzles/official")).split("/")[0]
    rec = bench.get((zone, f.name))
    if not rec or rec.get("status") == "PASS": continue
    env = {**os.environ, "RSOLVER_TIMEOUT_MS": "40000"}
    try:
        out = subprocess.run([BIN], stdin=open(f,"rb"), capture_output=True, timeout=50, env=env).stdout
        r = json.loads(out.decode().strip().splitlines()[-1])
    except subprocess.TimeoutExpired:
        r = {"solved": False, "elapsed_ms": 50000}
    except Exception:
        panic += 1; continue
    tested += 1
    if r.get("solved"):
        new_pass += 1; print(f"  NEW PASS {f.name} {r.get('elapsed_ms')}ms rules={sorted(rules)}", flush=True)
    else:
        still_fail += 1
    if tested >= 60: break
print(f"\n=== tested {tested} K-locked FAIL: NEW={new_pass} still_fail={still_fail} panic={panic} ===", flush=True)
