import json, subprocess, time, sys

rsolver = "rsolver/target/release/rsolver.exe"
tests = [
    ("5x5-compass-rose-watch", "puzzles/reference/5x5-compass-rose-watch.json"),
    ("7x7-area-sizesep", "puzzles/reference/7x7-area-sizesep.json"),
    ("7x7m3-compass", "puzzles/reference/7x7m3-compass.json"),
    ("4x4-shape", "puzzles/reference/4x4-shape.json"),
    ("2x4-diff-ineq-area", "puzzles/reference/2x4-diff-ineq-area.json"),
]
for name, path in tests:
    data = json.load(open(path))
    t0 = time.perf_counter()
    r = subprocess.run([rsolver], input=json.dumps(data), capture_output=True, text=True, timeout=30)
    ms = int((time.perf_counter() - t0) * 1000)
    sol = json.loads(r.stdout)
    print(f"{name:30s} solved={str(sol['solved']):5s} {ms:5d}ms  regions={len(sol.get('regions',[]))}  {sol.get('error_message','')}")
