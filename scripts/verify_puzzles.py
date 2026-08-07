"""Thin wrapper around benchmark_rust_solver.py for backwards compatibility.

All logic lives in benchmark_rust_solver.py now — this script just forwards
arguments.  The only difference is the default timeout (30 s here, 20 s in
benchmark_rust_solver.py) to preserve existing caller expectations.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

if __name__ == "__main__":
    from scripts.benchmark_rust_solver import main as bench_main
    # Override default timeout to 30 s for backwards compat
    import scripts.benchmark_rust_solver as bm
    # Inject the higher default via argv rewrite if --timeout wasn't passed
    argv = sys.argv[1:]
    if "--timeout" not in " ".join(argv):
        argv.insert(0, "--timeout")
        argv.insert(1, "30")
    sys.argv[1:] = argv
    bench_main()
