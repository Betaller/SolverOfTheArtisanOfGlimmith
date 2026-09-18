#!/usr/bin/env python3
"""Static pre-scan propagation probe for puzzle_piece / mixed / different / shape_pool.

Motivated by doc 25: "a size-1 piece is a deterministic fill; two adjacent cells
pinned to the same piece shape must be one region; run the pre-scan repeatedly".

The probe models every internal edge between two fillable cells as a variable in
{cut, same, unknown} and iterates the sound deductions below to a fixpoint, then
reports how much each puzzle gets forced.

Rules
-----
R0_pool_domain   shape_pool gives every cell a domain of possible region sizes
                 (and, under mixed/different, a set of forbidden shape keys);
                 a singleton size domain pins the region size.
R1_monomino      a cell pinned to a 1-cell shape is a full region -> all its
                 incident edges are cut.
R1_size2 / comp  a cell pinned to size s: the region is a connected s-set
                 containing it; if the open component has exactly s cells it is
                 sealed (inside = same, outside = cut), if it has fewer -> unsat.
R2_same_key      mixed/different: two adjacent cells pinned to the same shape key
                 cannot be two regions -> their edge is 'same'.
R3_forbid_key    mixed/different: a sealed region forbids its shape key for its
                 (mixed) neighbours / for everyone else (different).
R3b_cut_forbid   same, across an edge that is already cut.
R4_need_partner  a cell whose region size is known >= 2 and that has exactly one
                 open neighbour must merge with it.
R5_consistency   a sealed component's shape key must match every pinned key inside.

Usage
-----
    python scripts/exp_shape_prescan.py scan [--bench results/bench/<x>.jsonl]
    python scripts/exp_shape_prescan.py exp 1435 0994   # control vs +forced-cuts

`exp` writes variant puzzles (forced cut edges encoded as pre-drawn boundaries)
and solves both with the same rsolver binary.  Forced *same* edges cannot be
encoded: the puzzle JSON has no same-region edge constraint (io.rs:46-51), so
`exp` measures the cut-only subset of the pre-scan.
"""

from __future__ import annotations

import argparse
import collections
import glob
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_BENCH = ROOT / "results/bench/20260902_a8172ff_full.jsonl"
KEY_RULES = {"puzzle_piece", "mixed", "different", "same", "shape_pool", "precise"}
UNIT_MS = 40_000
WALL_SLACK = 4 * 1.2 + 30 / 40  # mirrors RustSolver.RUST_PARTS * SLACK


# ── puzzle loading ─────────────────────────────────────────────────────────────
def _dihedral_normalize(cells, rot: int, refl: bool) -> list[tuple[int, int]]:
    """Apply one dihedral transform (reflection + `rot` 90° turns) and normalize to origin."""
    t = []
    for r, c in cells:
        rr, cc = r, c
        if refl:
            cc = -cc
        for _ in range(rot):
            rr, cc = (-cc, rr)
        t.append((rr, cc))
    mr = min(x[0] for x in t)
    mc = min(x[1] for x in t)
    return sorted((x[0] - mr, x[1] - mc) for x in t)


def dihedral_key(cells):
    """Same canonicalisation as rsolver::shapes::dihedral_key (8 transforms, min)."""
    best = None
    for rot in range(4):
        for refl in (False, True):
            s = repr(_dihedral_normalize(cells, rot, refl))
            if best is None or s < best:
                best = s
    return best


def _extract_blocked_and_patterns(d: dict):
    blocked, patterns = set(), {}
    for c in d["cells"]:
        rc = (c["row"], c["col"])
        if c.get("blocked"):
            blocked.add(rc)
        if c.get("shape_pattern") is not None:
            pat = [(c["row"] + p[0], c["col"] + p[1]) for p in c["shape_pattern"]]
            patterns[rc] = (dihedral_key(pat), len(pat))
    return blocked, patterns


def _extract_pre(d: dict, blocked: set) -> set:
    pre = set()
    for e in d.get("edges", []):
        if not e.get("is_boundary"):
            continue
        a, b = (e["r1"], e["c1"]), (e["r2"], e["c2"])
        if a in blocked or b in blocked:
            continue
        pre.add(frozenset((a, b)))
    return pre


def _extract_pool(d: dict) -> dict:
    pool = {}
    for r in d.get("rules", []):
        if r.get("type") == "shape_pool":
            for s in r.get("params", {}).get("shapes", []):
                pool[dihedral_key([tuple(x) for x in s])] = len(s)
    if not pool and d.get("shape_pool"):
        for s in d["shape_pool"]:
            pool[dihedral_key([tuple(x) for x in s])] = len(s)
    return pool


def _extract_rules_prec(d: dict) -> tuple[set, object]:
    rules = {r.get("type") for r in d.get("rules", [])}
    prec = next(
        (r.get("params", {}).get("area") for r in d.get("rules", []) if r.get("type") == "precise"),
        None,
    )
    return rules, prec


def load(path: Path):
    with open(path) as fh:
        d = json.load(fh)
    h, w = d["grid"]["height"], d["grid"]["width"]
    blocked, patterns = _extract_blocked_and_patterns(d)
    fill = [(r, c) for r in range(h) for c in range(w) if (r, c) not in blocked]
    pre = _extract_pre(d, blocked)
    pool = _extract_pool(d)
    rules, prec = _extract_rules_prec(d)
    return d, fill, blocked, patterns, pre, pool, rules, prec


# ── propagation ────────────────────────────────────────────────────────────────
class Ctx:
    def _build_edges(self, fill, pre) -> dict[frozenset, str | None]:
        edges: dict[frozenset, str | None] = {}
        for rc in fill:
            for nb in self._raw(rc):
                edges.setdefault(frozenset((rc, nb)), None)
        for e in pre:
            if e in edges:
                edges[e] = "cut"
        return edges

    def _init_known(self, patterns, rules, fill, prec) -> tuple[dict, dict]:
        known_size = {rc: s for rc, (_, s) in patterns.items()}
        known_key = {rc: k for rc, (k, _) in patterns.items()}
        if prec is not None and "precise" in rules:
            for rc in fill:
                known_size[rc] = prec
        if "same" in rules and known_key:
            k0 = next(iter(known_key.values()))
            for rc in fill:
                known_key[rc] = k0
                known_size[rc] = len(eval(k0))  # key is a repr of a cell list
        return known_size, known_key

    def __init__(self, fill, pre, patterns, pool, rules, prec):
        self.fill = set(fill)
        self.fill_list = list(fill)
        self.edges = self._build_edges(fill, pre)
        self.pool = pool
        self.rules = rules
        self.patterns = patterns if "puzzle_piece" in rules else {}
        self.forbidden: dict[tuple, set] = collections.defaultdict(set)
        self.sealed: dict[tuple, tuple] = {}
        self.unsat = False
        self.fired: collections.Counter = collections.Counter()
        self.known_size, self.known_key = self._init_known(self.patterns, rules, fill, prec)

    def _raw(self, rc):
        r, c = rc
        return [
            (nr, nc)
            for nr, nc in ((r + 1, c), (r - 1, c), (r, c + 1), (r, c - 1))
            if (nr, nc) in self.fill
        ]

    def open_nbrs(self, rc):
        return [nb for nb in self._raw(rc) if self.edges[frozenset((rc, nb))] != "cut"]

    def component(self, rc):
        seen, stack = {rc}, [rc]
        while stack:
            x = stack.pop()
            for nb in self.open_nbrs(x):
                if nb not in seen:
                    seen.add(nb)
                    stack.append(nb)
        return seen

    def seal(self, comp):
        for x in comp:
            for nb in self._raw(x):
                e = frozenset((x, nb))
                if nb in comp:
                    if self.edges[e] == "cut":
                        self.unsat = True
                        return
                    self.edges[e] = "same"
                else:
                    if self.edges[e] == "same":
                        self.unsat = True
                        return
                    self.edges[e] = "cut"
        key = dihedral_key(sorted(comp))
        for x in comp:
            self.sealed[x] = (set(comp), key)

    def min_size(self, rc):
        if rc in self.known_size:
            return self.known_size[rc]
        if not self.pool:
            return 1
        allowed = set(self.pool) - self.forbidden[rc]
        if not allowed:
            self.unsat = True
            return 1
        return min(self.pool[k] for k in allowed)

    # ── fixpoint rule steps (each returns whether it changed the state) ──
    def _rule_pool_domain(self) -> bool:
        changed = False
        if not self.pool:
            return changed
        for rc in self.fill_list:
            if rc in self.known_size or rc in self.sealed:
                continue
            allowed = set(self.pool) - self.forbidden[rc]
            if not allowed:
                self.unsat = True
                continue
            sizes = {self.pool[k] for k in allowed}
            if len(sizes) == 1:
                self.known_size[rc] = next(iter(sizes))
                self.fired["R0_pool_domain"] += 1
                changed = True
        return changed

    def _rule_seal(self) -> bool:
        changed = False
        for rc in sorted(self.known_size):
            if rc in self.sealed:
                continue
            s = self.known_size[rc]
            if s == 1:
                self.seal({rc})
                self.fired["R1_monomino"] += 1
                changed = True
                continue
            comp = self.component(rc)
            if len(comp) < s:
                self.unsat = True
            elif len(comp) == s:
                self.seal(comp)
                self.fired["R1_comp_seal"] += 1
                changed = True
            elif s == 2:
                cand = self.open_nbrs(rc)
                if len(cand) == 1:
                    self.seal({rc, cand[0]})
                    self.fired["R1_size2"] += 1
                    changed = True
                elif not cand:
                    self.unsat = True
        return changed

    def _rule_same_key(self) -> bool:
        changed = False
        for e in list(self.edges):
            if self.edges[e] == "cut":
                continue
            a, b = tuple(e)
            ka = self.known_key.get(a)
            if ka is not None and ka == self.known_key.get(b) and self.edges[e] != "same":
                self.edges[e] = "same"
                self.fired["R2_same_key_merge"] += 1
                changed = True
        return changed

    def _r3_targets(self, comp, diff: bool) -> list:
        if diff:
            return [c for c in self.fill_list if c not in comp]
        targets = []
        for x in comp:
            for nb in self._raw(x):
                if nb not in comp and self.edges[frozenset((x, nb))] != "cut":
                    targets.append(nb)
        return targets

    def _rule_forbid_key(self, diff: bool) -> bool:
        changed = False
        for _rc, (comp, key) in list(self.sealed.items()):
            for nb in self._r3_targets(comp, diff):
                if nb in self.sealed or key in self.forbidden[nb]:
                    continue
                self.forbidden[nb].add(key)
                self.fired["R3_forbid_key"] += 1
                changed = True
        return changed

    def _rule_cut_forbid(self) -> bool:
        changed = False
        for e, st in list(self.edges.items()):
            if st != "cut":
                continue
            a, b = tuple(e)
            for x, y in ((a, b), (b, a)):
                k = self.known_key.get(x)
                if k is None or y in self.sealed or k in self.forbidden[y]:
                    continue
                self.forbidden[y].add(k)
                self.fired["R3b_cut_forbid"] += 1
                changed = True
        return changed

    def _rule_need_partner(self) -> bool:
        changed = False
        for rc in self.fill_list:
            if rc in self.sealed:
                continue
            if self.min_size(rc) >= 2:
                cand = self.open_nbrs(rc)
                if not cand:
                    self.unsat = True
                elif len(cand) == 1 and self.edges[frozenset((rc, cand[0]))] != "same":
                    self.edges[frozenset((rc, cand[0]))] = "same"
                    self.fired["R4_need_partner"] += 1
                    changed = True
        return changed

    def _rule_consistency(self) -> None:
        for _rc, (comp, key) in list(self.sealed.items()):
            for x in comp:
                k = self.known_key.get(x)
                if k is not None and k != key:
                    self.unsat = True


def propagate(ctx: Ctx) -> Ctx:
    mixed = "mixed" in ctx.rules
    diff = "different" in ctx.rules
    for _ in range(1000):
        changed = False
        changed |= ctx._rule_pool_domain()
        changed |= ctx._rule_seal()
        if mixed or diff:
            changed |= ctx._rule_same_key()
            changed |= ctx._rule_forbid_key(diff)
            changed |= ctx._rule_cut_forbid()
        changed |= ctx._rule_need_partner()
        ctx._rule_consistency()
        if not changed:
            break
    return ctx


def stats(ctx: Ctx, n_pre: int) -> dict:
    return {
        "forced_cut": sum(1 for s in ctx.edges.values() if s == "cut") - n_pre,
        "forced_same": sum(1 for s in ctx.edges.values() if s == "same"),
        "unknown": sum(1 for s in ctx.edges.values() if s is None),
        "edges": len(ctx.edges),
        "sealed_cells": len({c for comp, _ in ctx.sealed.values() for c in comp}),
        "unsat": ctx.unsat,
        "fired": dict(ctx.fired),
    }


def iter_puzzles():
    for f in sorted(glob.glob(str(ROOT / "puzzles/official/**/*.json"), recursive=True)):
        p = Path(f)
        if p.name.startswith("_") or any(part.endswith("-answer") for part in p.parts):
            continue
        yield p


# ── commands ───────────────────────────────────────────────────────────────────
def _load_bench(path: str) -> dict:
    bench = {}
    with open(path) as fh:
        for line in fh:
            r = json.loads(line)
            bench[r["file"]] = r
    return bench


def _scan_row(p: Path, bench: dict) -> tuple[dict | None, collections.Counter]:
    rel = str(p.relative_to(ROOT))
    d, fill, blocked, patterns, pre, pool, rules, prec = load(p)
    if not (KEY_RULES & rules):
        return None, collections.Counter()
    ctx = propagate(Ctx(fill, pre, patterns, pool, rules, prec))
    b = bench.get(rel, {})
    n_pre = len([e for e in pre if e in ctx.edges])
    row = dict(
        file=rel,
        rules=",".join(sorted(KEY_RULES & rules)),
        cells=len(fill),
        npat=len(patterns),
        status=b.get("status", "?"),
        ms=b.get("elapsed_ms", -1),
        **stats(ctx, n_pre),
    )
    return row, ctx.fired


def _print_scan(rows, fired, args) -> None:
    print("rule firings:")
    for k, v in fired.most_common():
        print(f"  {k:<20} {v}")
    aff = [r for r in rows if r["forced_cut"] or r["forced_same"]]
    status_counts = dict(collections.Counter(r["status"] for r in aff))
    print(f"\naffected {len(aff)}/{len(rows)}  status={status_counts}")
    for r in sorted(aff, key=lambda x: -(x["forced_cut"] + x["forced_same"]))[: args.top]:
        print(
            f"  {r['status']:4} {r['ms']:>6}ms {r['file']:<56} {r['rules']:<38} "
            f"cells={r['cells']:>3} pat={r['npat']:>2} cut+{r['forced_cut']:>3} "
            f"same+{r['forced_same']:>3} sealed={r['sealed_cells']:>3} "
            f"unk={r['unknown']:>4}/{r['edges']:>4}"
        )
    print("unsat:", sum(1 for r in rows if r["unsat"]))
    if args.json:
        with open(args.json, "w") as fh:
            json.dump(rows, fh)


def cmd_scan(args):
    bench = _load_bench(args.bench)
    rows, fired = [], collections.Counter()
    for p in iter_puzzles():
        row, row_fired = _scan_row(p, bench)
        if row is None:
            continue
        rows.append(row)
        fired += row_fired
    _print_scan(rows, fired, args)


def run(binary: Path, puzzle: dict, unit_ms: int):
    env = dict(os.environ, RSOLVER_TIMEOUT_MS=str(unit_ms))
    t0 = time.time()
    try:
        proc = subprocess.run(
            [str(binary)],
            input=json.dumps(puzzle),
            capture_output=True,
            text=True,
            env=env,
            timeout=unit_ms / 1000 * WALL_SLACK,
        )
    except subprocess.TimeoutExpired:
        return {"solved": False, "wall": int((time.time() - t0) * 1000), "err": "wall-timeout"}
    wall = int((time.time() - t0) * 1000)
    try:
        out = json.loads(proc.stdout)
    except Exception:
        return {"solved": False, "wall": wall, "err": f"unparseable: {proc.stdout[:120]}"}
    return {
        "solved": bool(out.get("solved")),
        "wall": wall,
        "ms": out.get("elapsed_ms"),
        "solver": out.get("solver"),
        "attempts": [(a["solver"], a["status"], a["elapsed_ms"]) for a in out.get("attempts", [])],
    }


def _name_match(rel: str, names) -> bool:
    return any(n in rel for n in names)


def _forced_edge_counts(ctx: Ctx, pre: set) -> tuple[int, int]:
    n_pre = len([e for e in pre if e in ctx.edges])
    cut = sum(1 for s in ctx.edges.values() if s == "cut") - n_pre
    same = sum(1 for s in ctx.edges.values() if s == "same")
    return cut, same


def _collect_targets(args) -> tuple[Path, list]:
    targets = []
    for p in iter_puzzles():
        rel = str(p.relative_to(ROOT))
        if args.names and not _name_match(rel, args.names):
            continue
        d, fill, blocked, patterns, pre, pool, rules, prec = load(p)
        if not (KEY_RULES & rules):
            continue
        ctx = propagate(Ctx(fill, pre, patterns, pool, rules, prec))
        cut, same = _forced_edge_counts(ctx, pre)
        if not args.names and not (cut or same):
            continue
        targets.append((rel, d, ctx, blocked))
    return Path(args.binary), targets


def _apply_variant_edges(d: dict, ctx: Ctx, blocked: set) -> int:
    have = {
        (e["r1"], e["c1"], e["r2"], e["c2"]) for e in d.get("edges", []) if e.get("is_boundary")
    }
    added = 0
    for e, st in ctx.edges.items():
        if st != "cut":
            continue
        a, b = tuple(e)
        key = (a[0], a[1], b[0], b[1])
        if key in have or (b[0], b[1], a[0], a[1]) in have or a in blocked or b in blocked:
            continue
        d.setdefault("edges", []).append(
            {"r1": a[0], "c1": a[1], "r2": b[0], "c2": b[1], "is_boundary": True}
        )
        added += 1
    return added


def cmd_exp(args):
    binary, targets = _collect_targets(args)
    for rel, d, ctx, blocked in targets:
        added = _apply_variant_edges(d, ctx, blocked)
        with open(ROOT / rel) as fh:
            orig = json.load(fh)
        ctrl, var = run(binary, orig, args.unit_ms), run(binary, d, args.unit_ms)
        print(
            f"{rel:<56} added={added:>3} | ctrl solved={ctrl['solved']!s:<5} {ctrl.get('ms')}ms "
            f"{ctrl.get('solver')} | variant solved={var['solved']!s:<5} {var.get('ms')}ms "
            f"{var.get('solver')}",
            flush=True,
        )
        print("      ctrl:", ctrl.get("attempts") or ctrl.get("err"), flush=True)
        print("      var :", var.get("attempts") or var.get("err"), flush=True)


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("scan")
    s.add_argument("--bench", default=str(DEFAULT_BENCH))
    s.add_argument("--json")
    s.add_argument("--top", type=int, default=40)
    s.set_defaults(func=cmd_scan)
    e = sub.add_parser("exp")
    e.add_argument("names", nargs="*")
    e.add_argument("--binary", default=str(ROOT / "rsolver/target/release/rsolver"))
    e.add_argument("--unit-ms", type=int, default=UNIT_MS)
    e.set_defaults(func=cmd_exp)
    args = ap.parse_args()
    args.func(args)


if __name__ == "__main__":
    sys.exit(main())
