"""
Verify all puzzles solve correctly.

Usage:
    python scripts/verify_puzzles.py
    python scripts/verify_puzzles.py --dir puzzles/official --timeout 30 -j 4
"""
from __future__ import annotations

import json
import sys
import time
import glob
import argparse
import concurrent.futures
from dataclasses import dataclass, field
from pathlib import Path

sys.path.insert(0, '.')

from src.io.puzzle_codec import dict_to_puzzle
from src.solver.base import default_router
from src.validation.validator import IndependentValidator, solution_to_board


@dataclass
class PuzzleResult:
    name: str
    solved: bool
    steps: int
    elapsed_ms: int
    validated: bool
    error: str | None = None
    solver_used: str | None = None
    attempts: list[str] = field(default_factory=list)


def test_one(args: tuple[str, float, bool]) -> PuzzleResult:
    path, timeout, log_failures = args
    name = path.replace('\\', '/').split('/')[-1]
    try:
        with open(path, encoding='utf-8') as f:
            data = json.load(f)
        puzzle = dict_to_puzzle(data)

        if not puzzle.rules:
            return PuzzleResult(name, True, 0, 0, True)

        # run the same solver chain the app uses (Rust → ExactCover → Rose →
        # Backtrack → FallbackDLX).  The router independently verifies every
        # solver's answer and switches on wrong results.
        router = default_router()
        start = time.monotonic()
        solution = router.route(puzzle, timeout=timeout, puzzle_name=name)
        t_ms = int((time.monotonic() - start) * 1000)

        attempts = [f"{a.solver_name}({'ok' if a.solved else 'fail'})"
                    for a in router.attempts]
        solver_used = next(
            (a.solver_name for a in router.attempts if a.solved), None
        )

        if not solution.solved:
            fillable = sum(1 for c in puzzle.cells if not c.blocked)
            pool_areas = set()
            if puzzle.has_rule('shape_pool') and puzzle.get_rule('shape_pool') is not None:
                pool = puzzle.get_rule('shape_pool').params.get('shapes', [])
                pool_areas = {s.area for s in pool}
            hint = ''
            if pool_areas and fillable % max(pool_areas) != 0:
                hint = f' [fillable={fillable} not divisible by pool_area={max(pool_areas)}]'
            return PuzzleResult(name, False, solution.steps_taken, t_ms, False,
                                solution.error_message or '无解' + hint,
                                solver_used, attempts)

        # independent re-verification (decoupled from solver rule checks)
        board = solution_to_board(puzzle, solution)
        val = IndependentValidator().validate(puzzle, board)
        return PuzzleResult(name, True, solution.steps_taken, t_ms, val.solved,
                            "; ".join(val.errors[:3]) if not val.solved else None,
                            solver_used, attempts)
    except Exception as e:
        return PuzzleResult(name, False, 0, 0, False, str(e))


def main():
    parser = argparse.ArgumentParser(description="验证所有谜题都能正常求解")
    parser.add_argument("--dir", default="puzzles", help="谜题目录 (默认 puzzles)")
    parser.add_argument("--timeout", type=float, default=30, help="每题超时秒数")
    parser.add_argument("-j", "--jobs", type=int, default=0, help="并行数 (默认 CPU 核心数)")
    args = parser.parse_args()

    # 跳过元数据/索引文件（如 `_index.json`）与官方解 answer 目录（`*-answer`）。
    # 它们描述谜题集/官方答案，不是可解谜题。
    files = sorted(
        f for f in glob.glob(f"{args.dir}/**/*.json", recursive=True)
        if not Path(f).name.startswith("_")
        and not any(part.endswith("-answer") for part in Path(f).parts)
    )
    if not files:
        print(f"在 {args.dir}/ 下未找到 .json 文件")
        sys.exit(1)

    total = len(files)
    passed = 0
    failed: list[PuzzleResult] = []

    print(f"共 {total} 个谜题，并行 {'CPU核数' if args.jobs<=0 else args.jobs} 线程\n")

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs or None) as pool:
        fut_map = {pool.submit(test_one, (f, args.timeout, True)): f for f in files}
        done = 0
        for fut in concurrent.futures.as_completed(fut_map):
            done += 1
            r = fut.result()
            status = "PASS" if r.solved and r.validated else "FAIL"
            if r.solved and r.validated:
                passed += 1
            else:
                failed.append(r)
            print(f"[{done:>3}/{total}] {status} {r.name:<20} "
                  f"via={r.solver_used or '-':<12} "
                  f"steps={r.steps:<6} time={r.elapsed_ms:>5}ms"
                  f"{'  ' + r.error if r.error else ''}")

    print(f"\n结果: {passed}/{total} 通过")
    if failed:
        print(f"失败 {len(failed)} 个:")
        for r in failed:
            print(f"  {r.name}: {r.error}")
        sys.exit(1)
    print("全部验证通过!")


if __name__ == "__main__":
    main()
