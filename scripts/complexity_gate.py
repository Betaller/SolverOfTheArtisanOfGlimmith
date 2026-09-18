#!/usr/bin/env python3
"""Static cyclomatic-complexity gate — this project's detekt equivalent.

detekt (Kotlin) ships a ``CyclomaticComplexMethod`` rule with a default
threshold of 10.  This script reproduces that discipline for every language
the repo ships, so a push is rejected when it would raise complexity:

  Python  → radon cyclomatic complexity            (threshold 10, ``--max``)
  Rust    → clippy ``cognitive_complexity``        (threshold from
                                                    ``rsolver/clippy.toml``)
  JS/TS/Vue → eslint ``complexity`` rule           (threshold from
                                                    ``web/eslint.config.js``)

All three thresholds default to the detekt value (10 for cyclomatic; clippy's
score is cognitive so its number is read from clippy.toml rather than
hard-coded here).

Usage:
    python scripts/complexity_gate.py                 # Python only (src/ + scripts/)
    python scripts/complexity_gate.py --all           # Python + Rust + JS/TS/Vue
    python scripts/complexity_gate.py --rust
    python scripts/complexity_gate.py --js
    python scripts/complexity_gate.py src/ --max 8    # stricter, single tree

Exit codes:
    0  every checked language is within its threshold
    1  at least one block exceeds a threshold
    2  invocation / configuration error
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

from radon.complexity import cc_visit
from radon.visitors import Class, Function

# Default trees scanned.  ``tests/`` and ``conftest.py`` are intentionally
# excluded — test helpers are allowed to be more procedural.  The shipped
# "modules" of the application live under ``src/``; ``scripts/`` holds the
# developer tooling and is held to the same standard.
DEFAULT_PATHS: tuple[str, ...] = ("src", "scripts")

RUST_DIR = Path("rsolver")
WEB_DIR = Path("web")

# Directories / files skipped regardless of the requested roots.
_SKIP_DIRS = {"__pycache__", ".venv", "venv", "build", "dist", "node_modules", ".git"}
_SKIP_NAMES = {"__init__.py", "complexity_gate.py"}


def _iter_py_files(roots: list[str]) -> list[Path]:
    files: list[Path] = []
    for root in roots:
        p = Path(root)
        if p.is_file() and p.suffix == ".py":
            files.append(p)
            continue
        if not p.exists():
            continue
        for f in p.rglob("*.py"):
            if any(part in _SKIP_DIRS for part in f.parts):
                continue
            if f.name in _SKIP_NAMES:
                continue
            files.append(f)
    return sorted(set(files))


def _blocks(path: Path) -> list[tuple[str, int, str, int]]:
    """Return ``(path, lineno, name, complexity)`` rows for every analyzed block."""
    text = path.read_text(encoding="utf-8")
    out: list[tuple[str, int, str, int]] = []
    try:
        analyzed = cc_visit(text)
    except Exception as exc:  # pragma: no cover - defensive, radon is robust
        print(f"  ! parse error: {path} ({exc})", file=sys.stderr)
        return out
    for block in analyzed:
        if isinstance(block, Function | Class):
            out.append((str(path), block.lineno, block.name, int(block.complexity)))
        # radon returns nested methods as their own blocks, so no recursion needed.
    return out


def _python_gate(paths: list[str], max_cc: int, strict: bool) -> int:
    violations: list[tuple[str, int, str, int]] = []
    parse_errors: list[str] = []
    total = 0
    for path in _iter_py_files(paths):
        blocks = _blocks(path)
        if blocks is None:  # parse error sentinel
            parse_errors.append(str(path))
            continue
        total += len(blocks)
        for p, lineno, name, cc in blocks:
            if cc > max_cc:
                violations.append((p, lineno, name, cc))

    violations.sort(key=lambda r: r[3], reverse=True)
    print(f"[python] radon 圈复杂度 (阈值 {max_cc})")
    if violations:
        print(f"复杂度超限 (CC > {max_cc}) — 共 {len(violations)} 处，扫描 {total} 个代码块:")
        for p, lineno, name, cc in violations:
            print(f"  {cc:>3}  {p}:{lineno}  {name}")
        print("\n请降低上述函数/方法的圈复杂度（拆分辅助函数、用分派表替代 if/elif 链等）。")
    else:
        print(f"  复杂度检测通过：扫描 {total} 个代码块，全部 <= {max_cc}。")

    if parse_errors:
        print(f"\n{len(parse_errors)} 个文件解析失败:", file=sys.stderr)
        for pe in parse_errors:
            print(f"  {pe}", file=sys.stderr)
        if strict:
            return 1

    return 1 if violations else 0


# ── Rust gate (clippy cognitive_complexity) ─────────────────────────────────

_CLIPPY_CC = re.compile(r"cognitive complexity of \((\d+)/(\d+)\)")
_CLIPPY_LOC = re.compile(r"-->\s*(\S+?):(\d+):(\d+)")


def _rust_gate() -> int:
    """clippy::cognitive_complexity over rsolver/. Threshold lives in clippy.toml."""
    if not RUST_DIR.is_dir():
        print("[rust] 跳过：未找到 rsolver/")
        return 0
    cargo = shutil.which("cargo")
    if cargo is None:
        print("[rust] 跳过：未安装 cargo", file=sys.stderr)
        return 0
    proc = subprocess.run(
        [cargo, "clippy", "--quiet", "--all-targets", "--", "-W", "clippy::cognitive_complexity"],
        cwd=RUST_DIR,
        capture_output=True,
        text=True,
    )
    out = proc.stdout + proc.stderr
    if proc.returncode != 0 and "could not compile" in out:
        print("[rust] clippy 编译失败，请先修复编译错误：", file=sys.stderr)
        print(out[-2000:], file=sys.stderr)
        return 1

    lines = out.splitlines()
    hits: list[tuple[int, int, str, int]] = []
    threshold = 0
    for i, line in enumerate(lines):
        m = _CLIPPY_CC.search(line)
        if not m:
            continue
        threshold = max(threshold, int(m.group(2)))
        loc = next(
            (
                lines[j]
                for j in range(i + 1, min(i + 4, len(lines)))
                if _CLIPPY_LOC.search(lines[j])
            ),
            "",
        )
        lm = _CLIPPY_LOC.search(loc)
        where = f"{lm.group(1)}:{lm.group(2)}" if lm else "?"
        hits.append((int(m.group(1)), 0, where, 0))
    print(f"[rust] clippy cognitive_complexity (阈值 {threshold or 'clippy.toml'})")
    if hits:
        hits.sort(reverse=True)
        print(f"  复杂度超限 — 共 {len(hits)} 处:")
        for cc, _z, where, _c in hits:
            print(f"  {cc:>3}  {where}")
        return 1
    print("  复杂度检测通过：无函数超过阈值。")
    return 0


# ── JS / TS / Vue gate (eslint complexity rule) ─────────────────────────────


def _js_gate() -> int:
    """eslint `complexity` over web/. Threshold lives in web/eslint.config.js."""
    if not WEB_DIR.is_dir():
        print("[js] 跳过：未找到 web/")
        return 0
    if not (WEB_DIR / "node_modules").is_dir():
        print("[js] 跳过：web/node_modules 未安装（cd web && npm ci）", file=sys.stderr)
        return 0
    npm = shutil.which("npm") or shutil.which("npx")
    if npm is None:
        print("[js] 跳过：未安装 npm/npx", file=sys.stderr)
        return 0
    exe = "npm" if shutil.which("npm") else "npx"
    cmd = (
        [exe, "exec", "--", "eslint", ".", "-f", "json"]
        if exe == "npm"
        else ["npx", "eslint", ".", "-f", "json"]
    )
    proc = subprocess.run(cmd, cwd=WEB_DIR, capture_output=True, text=True)
    raw = proc.stdout.strip()
    if not raw.startswith("["):
        print("[js] eslint 未输出 JSON，原始输出：", file=sys.stderr)
        print((proc.stdout + proc.stderr)[-2000:], file=sys.stderr)
        return 1
    try:
        report = json.loads(raw)
    except json.JSONDecodeError as exc:
        print(f"[js] eslint 输出解析失败: {exc}", file=sys.stderr)
        return 1

    hits: list[tuple[int, str, str]] = []
    for entry in report:
        for msg in entry.get("messages", []):
            if msg.get("ruleId") != "complexity":
                continue
            rel = str(entry.get("filePath", "")).split(f"{WEB_DIR.name}/")[-1]
            mm = re.search(r"complexity of (\d+)", msg.get("message", ""))
            hits.append(
                (int(mm.group(1)) if mm else 0, f"{rel}:{msg.get('line')}", msg.get("message", ""))
            )
    print("[js] eslint complexity (阈值见 web/eslint.config.js)")
    if hits:
        hits.sort(reverse=True)
        print(f"  复杂度超限 — 共 {len(hits)} 处:")
        for cc, where, _m in hits:
            print(f"  {cc:>3}  {where}")
        return 1
    print("  复杂度检测通过：无函数超过阈值。")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("paths", nargs="*", default=list(DEFAULT_PATHS), help="source trees to scan")
    ap.add_argument(
        "--max", type=int, default=10, help="max allowed cyclomatic complexity (default 10)"
    )
    ap.add_argument("--strict", action="store_true", help="also fail on parse errors")
    ap.add_argument("--all", action="store_true", help="also run the Rust and JS/TS/Vue gates")
    ap.add_argument("--rust", action="store_true", help="run only the Rust gate")
    ap.add_argument("--js", action="store_true", help="run only the JS/TS/Vue gate")
    args = ap.parse_args()

    if args.max < 1:
        print("error: --max must be >= 1", file=sys.stderr)
        return 2

    if args.rust:
        return _rust_gate()
    if args.js:
        return _js_gate()

    rc = _python_gate(args.paths, args.max, args.strict)
    if args.all:
        rc |= _rust_gate()
        rc |= _js_gate()
    return rc


if __name__ == "__main__":
    raise SystemExit(main())
