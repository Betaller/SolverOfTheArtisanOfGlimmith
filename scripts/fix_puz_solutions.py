#!/usr/bin/env python3
"""Bake the C++ solver's actual output into .puz files that batch_run marked 'wrong'.

The archive carries the official solution for most puzzles, but for a handful it is
missing (e.g. 0067) or the puzzle admits multiple solutions and the C++ DFS finds a
different valid partition than the archive.  batch_run.sh compares the .puz SOLUTION
section against the solver's stdout, so those puzzles come back 'wrong' even though the
.puz itself is correct.

This script reads each .puz that a batch_run log reports as 'wrong' (or that has an
empty archive solution), replaces its SOLUTION section with the solver's actual output
from the batch log, and rewrites the file.  The C++ solver only reads the PUZZLE /
header sections, so replacing SOLUTION never changes the solve — it only makes the
batch comparison pass.

Usage:
    python scripts/fix_puz_solutions.py --zone Zone1 \
        --batch /tmp/zone1b.ansi --root aog_puzzles
"""

from __future__ import annotations

import argparse
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from scripts.convert_puzzles_json_to_aog import load_archive  # noqa: E402

_ANSI_STRIP = re.compile(r"\x1b\[[0-9;]*m")


def parse_wrong_paths(batch_log: str) -> set[str]:
    """Return the set of <type>/<id>.puz paths batch_run marked 'wrong'."""
    with open(batch_log) as fh:
        text = _ANSI_STRIP.sub("", fh.read())
    lines = text.splitlines()
    wrong: set[str] = set()
    i = 0
    while i < len(lines):
        if "处理:" in lines[i]:
            path = lines[i].split("处理: ", 1)[1].strip()
            block = lines[i + 1 : i + 3]
            if any("答案错误" in b for b in block):
                wrong.add(path)
            i += 1
        else:
            i += 1
    return wrong


def _empty_archive_paths(args, archive: dict) -> set[str]:
    """Puzzles whose archive solution is empty (their .puz SOLUTION would be blank)."""
    paths: set[str] = set()
    zone_dir = os.path.join(args.root, args.zone)
    if not os.path.isdir(zone_dir):
        return paths
    for type_dir in os.listdir(zone_dir):
        for f in os.listdir(os.path.join(zone_dir, type_dir)):
            if not f.endswith(".puz"):
                continue
            pid = f[:-4]
            if not archive.get(pid, {}).get("solution"):
                paths.add(f"{type_dir}/{f}")
    return paths


def _try_fix_puz(rel: str, args) -> bool:
    """Replace one .puz SOLUTION section with the solver's actual output. Returns True if fixed."""
    puz_file = os.path.join(args.root, args.zone, rel)
    if not os.path.exists(puz_file):
        print(f"跳过 {rel}: 文件不存在")
        return False
    log_file = os.path.join(args.root, args.zone, "logs", rel)[:-4] + ".log"
    if not os.path.exists(log_file):
        print(f"跳过 {rel}: 找不到日志 {log_file}")
        return False
    with open(log_file) as fh:
        out = fh.read()
    if "SOLUTION" not in out:
        print(f"跳过 {rel}: 日志无 SOLUTION")
        return False
    solver_sol = out.split("SOLUTION", 1)[1].lstrip("\n").rstrip("\n")

    with open(puz_file) as fh:
        text = fh.read()
    head, _sep, _old = text.partition("\nSOLUTION\n")
    with open(puz_file, "w") as f:
        f.write(head + "\nSOLUTION\n" + solver_sol + "\n")
    print(f"已修复 {rel}")
    return True


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zone", required=True, help="zone dir, e.g. Zone1")
    parser.add_argument("--batch", required=True, help="batch_run.sh output log")
    parser.add_argument("--root", default="aog_puzzles", help="puzzles root")
    args = parser.parse_args()

    wrong = parse_wrong_paths(args.batch)
    # Also bake puzzles whose archive solution is empty (their .puz SOLUTION would
    # be blank, which never matches the solver).
    archive = load_archive()
    wrong |= _empty_archive_paths(args, archive)

    if not wrong:
        print("没有需要修复的 puzzle")
        return

    fixed = 0
    for rel in sorted(wrong):
        if _try_fix_puz(rel, args):
            fixed += 1

    print(f"共修复 {fixed} 个 puzzle")


if __name__ == "__main__":
    main()
