"""Benchmark aog (Rust) vs rsolver (Rust) on reference puzzles."""
import json
import subprocess
import sys
import time
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
AOG_BIN = PROJECT_ROOT / "third_party" / "aog" / "target" / "release" / "aog_solver.exe"
RSOLVER_BIN = PROJECT_ROOT / "rsolver" / "target" / "release" / "rsolver.exe"
AOG_SAMPLES = PROJECT_ROOT / "third_party" / "aog" / "samples"
REFERENCE_DIR = PROJECT_ROOT / "puzzles" / "reference"

# Map sample .txt files to reference .json files
SAMPLE_TO_REF = {
    "1x4-multi.txt": None,
    "2x2-shape.txt": "2x2-shape.json",
    "2x4-diff-ineq-area.txt": "2x4-diff-ineq-area.json",
    "4x4-shape.txt": "4x4-shape.json",
    "5x5-compass-rose-watch.txt": "5x5-compass-rose-watch.json",
    "7x7-area-sizesep.txt": "7x7-area-sizesep.json",
    "7x7-compass-solitude.txt": "7x7-compass-solitude.json",
    "7x7m1-gemini-delta-precut.txt": "7x7m1-gemini-delta-precut.json",
    "7x7m3-compass.txt": "7x7m3-compass.json",
    "7x7m9-diff-mismatch.txt": "7x7m9-diff-mismatch.json",
    "8x8-area.txt": "8x8-area.json",
    "8x8-ineq-watch-boxy.txt": "8x8-ineq-watch-boxy.json",
    "8x8m8-min3-max6-nonboxy-ineq.txt": "8x8m8-min3-max6-nonboxy-ineq.json",
    "8x10m22-watch.txt": "8x10m22-watch.json",
    "9x10-loopy-rose-watch.txt": "9x10-loopy-rose-watch.json",
    "10x10-poly-pali.txt": "10x10-poly-pali.json",
    "10x10m2-rose-watch.txt": "10x10m2-rose-watch.json",
    "11x11-poly-area.txt": "11x11-poly-area.json",
    "14x14-shape-gemini-delta.txt": "14x14-shape-gemini-delta.json",
    "28-shape.txt": "28-shape.json",
    "42-rose-match21.txt": "42-rose-match21.json",
    "61-compass.txt": "61-compass.json",
    "168-rose-match7.txt": "168-rose-match7.json",
}


def run_aog(txt_path: Path, timeout: float = 30.0) -> tuple[bool, int]:
    """Run aog solver on a .txt puzzle. Returns (solved, elapsed_ms)."""
    try:
        t0 = time.perf_counter()
        result = subprocess.run(
            [str(AOG_BIN), str(txt_path)],
            capture_output=True, text=True, timeout=timeout,
        )
        elapsed = int((time.perf_counter() - t0) * 1000)
        solved = "Unique" in result.stdout or "Multiple" in result.stdout
        return solved, elapsed
    except subprocess.TimeoutExpired:
        return False, int(timeout * 1000)
    except Exception as e:
        return False, 0


def run_rsolver(json_path: Path, timeout: float = 10.0) -> tuple[bool, int]:
    """Run rsolver on a .json puzzle. Returns (solved, elapsed_ms)."""
    try:
        puzzle_data = json.loads(json_path.read_text(encoding="utf-8"))
        t0 = time.perf_counter()
        result = subprocess.run(
            [str(RSOLVER_BIN)],
            input=json.dumps(puzzle_data),
            capture_output=True, text=True, timeout=timeout,
        )
        elapsed = int((time.perf_counter() - t0) * 1000)
        data = json.loads(result.stdout)
        return data.get("solved", False), elapsed
    except subprocess.TimeoutExpired:
        return False, int(timeout * 1000)
    except Exception:
        return False, 0


def main():
    results = []

    for txt_name, json_name in SAMPLE_TO_REF.items():
        txt_path = AOG_SAMPLES / txt_name
        if not txt_path.exists():
            continue

        print(f"{txt_name:40s} ", end="", flush=True)

        aog_solved, aog_ms = run_aog(txt_path, timeout=30.0)
        print(f"aog={'OK' if aog_solved else '--':3s} {aog_ms:5d}ms  ", end="", flush=True)

        if json_name:
            json_path = REFERENCE_DIR / json_name
            if json_path.exists():
                rs_solved, rs_ms = run_rsolver(json_path, timeout=10.0)
                print(f"rs={'OK' if rs_solved else '--':3s} {rs_ms:5d}ms", end="")
            else:
                rs_solved, rs_ms = False, 0
                print(f"rs=no_file", end="")
        else:
            rs_solved, rs_ms = False, 0
            print(f"rs=no_json ", end="")

        print()
        results.append((txt_name, aog_solved, aog_ms, rs_solved, rs_ms))

    print()
    print(f"{'Puzzle':40s} {'aog':>6s} {'aog ms':>8s} {'rs':>6s} {'rs ms':>8s}")
    print("-" * 72)
    aog_total, aog_passed = 0, 0
    rs_total, rs_passed = 0, 0
    for name, aog_s, aog_m, rs_s, rs_m in results:
        if aog_m > 0:
            aog_total += 1
            if aog_s:
                aog_passed += 1
        if rs_m > 0:
            rs_total += 1
            if rs_s:
                rs_passed += 1
        print(f"{name:40s} {'OK' if aog_s else '--':>6s} {aog_m:8d} {'OK' if rs_s else '--':>6s} {rs_m:8d}")
    print("-" * 72)
    print(f"aog: {aog_passed}/{aog_total} solved, rsolver: {rs_passed}/{rs_total} solved")


if __name__ == "__main__":
    main()
