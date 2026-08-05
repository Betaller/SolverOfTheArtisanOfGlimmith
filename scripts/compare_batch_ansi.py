#!/usr/bin/env python3
"""Compare a batch_run.sh output log against a reference .ansi log.

The two logs can never be byte-identical: batch_run.sh embeds per-puzzle wall-clock
timings and the reference was produced with a different worker count / machine.  This
script compares what matters instead — the ordered list of puzzle files and the status
reported for each (correct / timeout / wrong / no_solution / error).

Usage:
    python scripts/compare_batch_ansi.py --ref third_party/AoG_Solver/Zone1.ansi \
        --new /tmp/zone1_run.ansi
"""
from __future__ import annotations

import argparse
import re
import sys

_ANSI_STRIP = re.compile(r"\x1b\[[0-9;]*m")


def parse_log(text: str) -> list[tuple[str, str]]:
    """Return [(puzzle_path, status), ...] in the order batch_run.sh printed them."""
    text = _ANSI_STRIP.sub("", text)
    lines = text.splitlines()
    result: list[tuple[str, str]] = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if "处理:" not in line:
            i += 1
            continue
        path = line.split("处理: ", 1)[1].strip()
        status = None
        for j in range(i + 1, min(i + 4, len(lines))):
            if "答案正确" in lines[j]:
                status = "correct"
            elif "答案错误" in lines[j]:
                status = "wrong"
            elif "超时终止" in lines[j]:
                status = "timeout"
            elif "未输出 SOLUTION" in lines[j]:
                status = "no_solution"
            elif "运行失败" in lines[j]:
                status = "error"
            if status is not None:
                break
        if status is None:
            sys.exit(f"无法解析 {path!r} 的状态")
        result.append((path, status))
        i += 1
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ref", required=True, help="reference .ansi file")
    parser.add_argument("--new", required=True, help="new batch_run.sh output file")
    args = parser.parse_args()

    ref = parse_log(open(args.ref).read())
    new = parse_log(open(args.new).read())

    mismatches = []
    if len(ref) != len(new):
        mismatches.append(f"数量不同: ref={len(ref)} new={len(new)}")
    for i, ((r_path, r_status), (n_path, n_status)) in enumerate(zip(ref, new)):
        if r_path != n_path or r_status != n_status:
            mismatches.append(f"[{i}] ref=({r_path}, {r_status}) new=({n_path}, {n_status})")

    if not mismatches:
        print(f"OK: {len(ref)} 个 puzzle 的路径与状态全部一致 ({args.ref} vs {args.new})")
        return
    print(f"不匹配 {len(mismatches)} 处:")
    for m in mismatches[:40]:
        print("  " + m)


if __name__ == "__main__":
    main()
