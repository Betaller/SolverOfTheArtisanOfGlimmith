"""
Verify all puzzles solve correctly.

Usage:
    python scripts/verify_puzzles.py
    python scripts/verify_puzzles.py --dir puzzles/official --timeout 30 -j 4
"""
from __future__ import annotations

import argparse
import concurrent.futures
import glob
import json
import sys
import time
from contextlib import nullcontext
from dataclasses import dataclass, field
from pathlib import Path

sys.path.insert(0, '.')

from src.io.puzzle_codec import dict_to_puzzle
from src.solver.base import default_router
from src.validation.official_answer import matches_official_answer
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
    # True = partition equals the official answer; False = valid but different;
    # None = no official answer file (reference / user / aiGen puzzles).
    matches_official: bool | None = None


def test_batch(args: tuple[list[str], float]) -> list[tuple[str, PuzzleResult]]:
    """Solve a chunk of puzzles in ONE rsolver `--batch` subprocess (via
    `RustSolver.solve_batch`), then independently re-verify each with the same
    `IndependentValidator` used by `test_one`.  Returns `(path, PuzzleResult)`
    pairs preserving input order.
    """
    paths, timeout = args
    results: list[tuple[str, PuzzleResult]] = []
    try:
        from src.solver.rust_solver import RustSolver

        solver = RustSolver()
        to_solve: list[tuple[str, str, object]] = []  # (name, path, puzzle)
        for path in paths:
            name = path.replace('\\', '/').split('/')[-1]
            try:
                with open(path, encoding='utf-8') as f:
                    data = json.load(f)
                puzzle = dict_to_puzzle(data)
            except Exception as e:
                results.append((path, PuzzleResult(name, False, 0, 0, False, str(e))))
                continue
            if not puzzle.rules:
                results.append((path, PuzzleResult(name, True, 0, 0, True)))
                continue
            to_solve.append((name, path, puzzle))

        solutions = solver.solve_batch([p for _, _, p in to_solve], timeout=timeout)
        for (name, path, puzzle), solution in zip(to_solve, solutions):
            if not solution.solved:
                results.append((path, PuzzleResult(
                    name, False, solution.steps_taken, solution.elapsed_ms,
                    False, solution.error_message or '无解', 'rust', ['rust(fail)'])))
                continue
            board = solution_to_board(puzzle, solution)
            val = IndependentValidator().validate(puzzle, board)
            if not val.solved:
                results.append((path, PuzzleResult(
                    name, True, solution.steps_taken, solution.elapsed_ms,
                    False, "; ".join(val.errors[:3]), 'rust', ['rust(ok)'])))
                continue
            matches = matches_official_answer(path, [r.cells for r in solution.regions])
            results.append((path, PuzzleResult(
                name, True, solution.steps_taken, solution.elapsed_ms,
                True, None, 'rust', ['rust(ok)'], matches_official=matches)))
    except Exception as e:
        for path in paths:
            name = path.replace('\\', '/').split('/')[-1]
            results.append((path, PuzzleResult(name, False, 0, 0, False, str(e))))
    return results


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
        if not val.solved:
            return PuzzleResult(name, True, solution.steps_taken, t_ms, False,
                                "; ".join(val.errors[:3]), solver_used, attempts)
        # official unique solution must match (None = no answer file)
        matches = matches_official_answer(path, [r.cells for r in solution.regions])
        return PuzzleResult(name, True, solution.steps_taken, t_ms, True,
                            None, solver_used, attempts, matches_official=matches)
    except Exception as e:
        return PuzzleResult(name, False, 0, 0, False, str(e))


def _path_meta(path: str) -> dict:
    """Extract zone/type/id from an official puzzle path, else dir + basename."""
    parts = path.replace('\\', '/').split('/')
    name = parts[-1]
    if len(parts) >= 5 and parts[-4] == 'official':
        return {'zone': parts[-3], 'type': parts[-2], 'id': name[:-5]}
    return {'zone': parts[-2] if len(parts) >= 3 else '', 'type': '', 'id': name[:-5]}


def _record(path: str, r: PuzzleResult) -> dict:
    """Serialize one result as a JSONL record close to ``scan_official_results.jsonl``."""
    meta = _path_meta(path)
    try:
        with open(path, encoding='utf-8') as f:
            data = json.load(f)
        grid, rules = data.get('grid'), data.get('rules', [])
    except Exception:
        grid, rules = None, []
    return {
        'file': path,
        'zone': meta['zone'],
        'type': meta['type'],
        'id': meta['id'],
        'grid': grid,
        'rules': rules,
        'status': 'PASS' if r.solved and r.validated else 'FAIL',
        'solved': r.solved,
        'validated': r.validated,
        'matches_official': r.matches_official,
        'elapsed_ms': r.elapsed_ms,
        'steps': r.steps,
        'solver_used': r.solver_used,
        'error': r.error,
    }


def main():
    parser = argparse.ArgumentParser(description="验证所有谜题都能正常求解")
    parser.add_argument("--dir", default="puzzles", help="谜题目录 (默认 puzzles)")
    parser.add_argument("--timeout", type=float, default=30, help="每题超时秒数")
    parser.add_argument("-j", "--jobs", type=int, default=0, help="并行数 (默认 CPU 核心数)")
    parser.add_argument("--batch", type=int, default=1,
                        help="每批谜题数，复用同一个 rsolver 子进程 (默认 1=逐题 spawn)")
    parser.add_argument("--rules", help="只验证包含该规则的谜题 (如 block)")
    parser.add_argument("--out", help="把每题结果追加写入该 JSONL 文件")
    args = parser.parse_args()

    # 跳过元数据/索引文件（如 `_index.json`）与官方解 answer 目录（`*-answer`）。
    # 它们描述谜题集/官方答案，不是可解谜题。
    files = sorted(
        f for f in glob.glob(f"{args.dir}/**/*.json", recursive=True)
        if not Path(f).name.startswith("_")
        and not any(part.endswith("-answer") for part in Path(f).parts)
    )
    if args.rules:
        kept = []
        for f in files:
            try:
                with open(f, encoding='utf-8') as fh:
                    data = json.load(fh)
                if any(r.get('type') == args.rules for r in data.get('rules', [])):
                    kept.append(f)
            except Exception:
                continue
        files = kept
        if not files:
            print(f"在 {args.dir}/ 下未找到含规则 '{args.rules}' 的谜题")
            sys.exit(1)
    if not files:
        print(f"在 {args.dir}/ 下未找到 .json 文件")
        sys.exit(1)

    total = len(files)
    passed = 0
    failed: list[PuzzleResult] = []

    print(f"共 {total} 个谜题，并行 {'CPU核数' if args.jobs<=0 else args.jobs} 线程\n")

    out_ctx = open(args.out, 'a', encoding='utf-8') if args.out else nullcontext()  # noqa: SIM115
    with (
        out_ctx as out_fh,
        concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs or None) as pool,
    ):
        def emit(path: str, r: PuzzleResult, done: int) -> None:
            nonlocal passed
            # solved & validated but partition ≠ official unique solution → DIFF
            diff = r.solved and r.validated and r.matches_official is False
            status = "DIFF" if diff else ("PASS" if r.solved and r.validated else "FAIL")
            if diff:
                r.error = (r.error or "") + " 解与官方题解不一致"
                failed.append(r)
            elif r.solved and r.validated:
                passed += 1
            else:
                failed.append(r)
            print(f"[{done:>3}/{total}] {status} {r.name:<20} "
                  f"via={r.solver_used or '-':<12} "
                  f"steps={r.steps:<6} time={r.elapsed_ms:>5}ms"
                  f"{'  ' + r.error if r.error else ''}")
            if out_fh is not None:
                out_fh.write(json.dumps(_record(path, r), ensure_ascii=False) + "\n")

        if args.batch > 1:
            chunks = [files[i:i + args.batch] for i in range(0, len(files), args.batch)]
            fut_map = {pool.submit(test_batch, (chunk, args.timeout)): chunk for chunk in chunks}
            done = 0
            for fut in concurrent.futures.as_completed(fut_map):
                for path, r in fut.result():
                    done += 1
                    emit(path, r, done)
        else:
            fut_map = {pool.submit(test_one, (f, args.timeout, True)): f for f in files}
            for done, fut in enumerate(concurrent.futures.as_completed(fut_map), 1):
                emit(fut_map[fut], fut.result(), done)

    print(f"\n结果: {passed}/{total} 通过")
    if failed:
        print(f"失败 {len(failed)} 个:")
        for r in failed:
            print(f"  {r.name}: {r.error}")
        sys.exit(1)
    print("全部验证通过!")


if __name__ == "__main__":
    main()
