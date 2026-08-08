#!/usr/bin/env python3
"""Test K-locked FAIL puzzles for A1 K-bounding value (skip known aog-hangs)."""
import json, pathlib, subprocess, os, sys
bench = {}
for line in open("results/bench/latest.jsonl"):
    r = json.loads(line.strip()); bench[(r.get("zone",""), r.get("name",""))] = r
SKIP = {"0833.json","1333.json","0749.json","0829.json","0875.json","C4-2.json","0213.json","0213nopad.json","0882.json","0826.json","0838.json","0999.json","0685.json","1320.json","1348.json"}
tested = new = 0
for f in sorted(pathlib.Path("puzzles/official").rglob("*.json")):
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
        if [c.get("symbol") for c in cells if c.get("symbol")]: is_k = True
    if not is_k: continue
    if f.name in SKIP: continue
    zone = str(f.relative_to("puzzles/official")).split("/")[0]
    rec = bench.get((zone, f.name))
    if not rec or rec.get("status") == "PASS": continue
    env = {**os.environ, "RSOLVER_TIMEOUT_MS": "25000"}
    try:
        out = subprocess.run(["./rsolver/target/release/rsolver"], stdin=open(f,"rb"), capture_output=True, timeout=28, env=env).stdout
        r = json.loads(out.decode().strip().splitlines()[-1])
    except subprocess.TimeoutExpired:
        r = {"solved": False, "elapsed_ms": 28000}
    except Exception as e:
        print(f"  {f.name}: ERR {e}", flush=True); continue
    tested += 1
    tag = "NEW PASS" if r.get("solved") else "fail"
    print(f"  {f.name}: {tag} {r.get('elapsed_ms')}ms rules={sorted(rules)}", flush=True)
    if r.get("solved"): new += 1
    if tested >= 15: break
print(f"\n=== tested {tested}: NEW={new} ===", flush=True)
