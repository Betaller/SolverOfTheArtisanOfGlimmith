#!/usr/bin/env python3
"""求解能力历史追踪：CI 趋势图的数据管线。

三个子命令（一条脚本，共享同一份 JSON schema）：

  seed   从 docs/official-puzzles-status.md 的「第一部分：进度」里程碑表解析
         历史基准点（仅取 1258 口径的全量官方题基准），生成初始
         docs/solver-history.json。
  append 解析一次 benchmark 日志里的 ``结果: X/Y 通过`` 汇总行，追加一个数据点。
  render 读取 docs/solver-history.json，生成：
         - docs/solver-history.png   （README 内嵌曲线，matplotlib）
         - site/index.html           （GitHub Pages 交互页，内联 SVG + hover）

JSON schema：

  {
    "unit":   "官方谜题 (puzzles/official, 排除 -answer 目录)",
    "total":  1258,                      # 最新一次运行的题数
    "latest": {"date","commit","passed","total","pct"},   # history 最后一项的镜像
    "history":[ {"date","commit","passed","total","pct"}, ... ]   # 时间升序
  }

shields.io 动态徽章读取 ``$.latest.passed`` / ``$.latest.pct``，所以 latest 字段
必须始终指向 history 的末尾（append 里由 hist[-1] 重算，勿手工维护）。

仅 seed/append/render 需要标准库；render 的 PNG 分支惰性 import matplotlib，
seed/append 在无 matplotlib 的环境（如某些 CI）也能独立运行。
"""

from __future__ import annotations

import argparse
import datetime
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

DEFAULT_JSON = REPO_ROOT / "docs" / "solver-history.json"
DEFAULT_PNG = REPO_ROOT / "docs" / "solver-history.png"
DEFAULT_HTML = REPO_ROOT / "site" / "index.html"
DEFAULT_MD = REPO_ROOT / "docs" / "official-puzzles-status.md"

OFFICIAL_TOTAL = 1258  # 官方题基准口径（puzzles/official 排除 -answer）


def _pct(passed: int, total: int) -> float:
    return round(passed * 100.0 / total, 2) if total else 0.0


def _load_json(path: Path) -> dict:
    if path.exists():
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    return {"unit": "官方谜题 (puzzles/official, 排除 -answer 目录)", "history": []}


def _save_json(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)
        f.write("\n")


# ── seed ────────────────────────────────────────────────────────────────────


def _find_hex(s: str) -> str | None:
    """7 位 hex（git 短 sha）。

    用 hex 前后向断言而非 ``\\b``：``\\b`` 在 ``_dfadfe3_`` 这种下划线分隔的
    文件名里不成立（下划线是 ``\\w``，没有词边界），会漏掉结果文件列里的 sha。
    """
    m = re.search(r"(?<![0-9a-f])[0-9a-f]{7}(?![0-9a-f])", s)
    return m.group(0) if m else None


def _find_backtick(s: str) -> str | None:
    m = re.search(r"`([^`]+)`", s)
    return m.group(1) if m else None


def _parse_md(path: Path) -> list[dict]:
    """解析里程碑表，返回按出现顺序排列的 1258 口径历史点。"""
    lines = path.read_text(encoding="utf-8").splitlines()
    in_section = False
    points: list[dict] = []
    for line in lines:
        s = line.strip()
        if s.startswith("## 第一部分"):
            in_section = True
            continue
        if s.startswith("## 第二部分"):
            break
        if not in_section or not s.startswith("|"):
            continue
        cells = [c.strip() for c in s.strip("|").split("|")]
        if len(cells) < 5:
            continue
        date = cells[0]
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", date):
            continue
        # 通过列（cells[4]）里的 ``N / M``；只取 1258 口径全量基准，避免混入
        # 1295 全量 verify、Zone 单区（301/312）、「待全量验证」等不同口径行。
        m = re.search(r"(\d+)\s*/\s*(\d+)", cells[4])
        if not m:
            continue
        passed, total = int(m.group(1)), int(m.group(2))
        if total != OFFICIAL_TOTAL:
            continue
        # commit：优先里程碑列的 hex 短 sha，其次结果文件列文件名里的 hex，再退到反引号标签。
        commit = _find_hex(cells[1]) or _find_hex(cells[2]) or _find_backtick(cells[1]) or ""
        points.append(
            {
                "date": date,
                "commit": commit,
                "passed": passed,
                "total": total,
                "pct": _pct(passed, total),
            }
        )
    return points


def cmd_seed(args: argparse.Namespace) -> None:
    md = Path(args.status_md)
    if not md.exists():
        raise SystemExit(f"未找到状态文档: {md}")
    out = Path(args.out)
    if out.exists() and not args.force:
        raise SystemExit(f"{out} 已存在，使用 --force 覆盖")
    points = _parse_md(md)
    if not points:
        raise SystemExit(f"未从 {md} 解析到任何 1258 口径基准点")
    data = {
        "unit": "官方谜题 (puzzles/official, 排除 -answer 目录)",
        "total": points[-1]["total"],
        "latest": points[-1],
        "history": points,
    }
    _save_json(out, data)
    print(f"seed: 从 {md} 解析 {len(points)} 个历史点 → {out}")
    for p in points:
        print(f"  {p['date']}  {p['commit']:<14} {p['passed']}/{p['total']}  ({p['pct']:.2f}%)")


# ── append ──────────────────────────────────────────────────────────────────


def _parse_bench_log(path: Path) -> tuple[int, int]:
    text = path.read_text(encoding="utf-8")
    m = re.search(r"结果:\s*(\d+)\s*/\s*(\d+)\s*通过", text)
    if not m:
        raise SystemExit(f"未在日志中找到「结果: X/Y 通过」汇总行: {path}")
    return int(m.group(1)), int(m.group(2))


def cmd_append(args: argparse.Namespace) -> None:
    log = Path(args.log)
    if not log.exists():
        raise SystemExit(f"未找到 benchmark 日志: {log}")
    out = Path(args.json)
    passed, total = _parse_bench_log(log)
    date = args.date or datetime.date.today().isoformat()
    point = {
        "date": date,
        "commit": args.commit or "",
        "passed": passed,
        "total": total,
        "pct": _pct(passed, total),
    }
    data = _load_json(out)
    hist = data.setdefault("history", [])
    # 同一 commit 去重（替换旧记录，last-wins，与 benchmark 的 baseline 语义一致）
    if point["commit"]:
        for i, p in enumerate(hist):
            if p.get("commit") == point["commit"]:
                hist[i] = point
                break
        else:
            hist.append(point)
    else:
        hist.append(point)
    data["total"] = hist[-1]["total"]
    data["latest"] = hist[-1]
    _save_json(out, data)
    print(
        f"append: {date}  {point['commit'] or '(no commit)'}  "
        f"{passed}/{total}  ({point['pct']:.2f}%)  →  {out}  (history={len(hist)})"
    )


# ── render: PNG（README 用）─────────────────────────────────────────────────


def _render_png(data: dict, png_path: Path) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    hist = data["history"]
    xs = list(range(len(hist)))
    ys = [p["pct"] for p in hist]

    # 参考调色板（light surface）：单序列蓝、墨色文字、弱化网格。
    surface = "#fcfcfb"
    ink = "#0b0b0b"
    secondary = "#52514e"
    muted = "#898781"
    grid = "#e1e0d9"
    series = "#2a78d6"

    fig, ax = plt.subplots(figsize=(8, 3.6), dpi=150)
    fig.patch.set_facecolor(surface)
    ax.set_facecolor(surface)

    ax.plot(
        xs,
        ys,
        color=series,
        linewidth=2,
        marker="o",
        markersize=5,
        markerfacecolor=series,
        markeredgecolor="white",
        markeredgewidth=1.2,
        zorder=3,
    )

    # 纵轴缩放到数据区间并留边，让「变化」可见；横轴留点呼吸。
    lo, hi = min(ys), max(ys)
    pad = max((hi - lo) * 0.5, 0.4)
    ax.set_ylim(lo - pad, hi + pad)
    ax.set_xlim(-0.45, len(hist) - 0.55)

    ax.set_xticks(xs)
    ax.set_xticklabels([p["date"][5:] for p in hist], color=muted, fontsize=9)
    ax.yaxis.grid(True, color=grid, linewidth=0.8)
    ax.xaxis.grid(False)
    for s in ax.spines.values():
        s.set_color(muted)
        s.set_linewidth(0.8)
    ax.tick_params(colors=muted, length=0, labelsize=9)

    ax.set_title("Official puzzle solve rate", color=ink, fontsize=12, loc="left", pad=10)
    ax.set_ylabel("solve rate (%)", color=secondary, fontsize=9)

    last = hist[-1]
    ax.annotate(
        f"{last['passed']}/{last['total']}  ({last['pct']:.2f}%)",
        xy=(xs[-1], ys[-1]),
        xytext=(xs[-1] - 1.15, ys[-1] + pad * 0.55),
        color=ink,
        fontsize=10,
        fontweight="bold",
    )
    ax.annotate(
        f"{hist[0]['pct']:.2f}%",
        xy=(xs[0], ys[0]),
        xytext=(xs[0] + 0.15, ys[0] - pad),
        color=secondary,
        fontsize=9,
    )

    fig.tight_layout()
    png_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(png_path, facecolor=surface, bbox_inches="tight")
    plt.close(fig)
    print(f"render: PNG → {png_path}")


# ── render: HTML（Pages 交互页）─────────────────────────────────────────────


def _svg_chart(hist: list[dict]) -> str:
    """内联 SVG 折线图（内联进 HTML，随页面 light/dark 一起换肤）。"""
    W, H = 840, 380
    L, R, T, B = 60, 24, 24, 40  # 左/右/上/下边距
    plot_w, plot_h = W - L - R, H - T - B
    ys = [p["pct"] for p in hist]
    lo, hi = min(ys), max(ys)
    span = (hi - lo) or 1.0
    pad = span * 0.35
    lo, hi = lo - pad, hi + pad

    def x(i: int) -> float:
        return L + (plot_w * i / (len(hist) - 1)) if len(hist) > 1 else L + plot_w / 2

    def y(v: float) -> float:
        return T + plot_h * (1 - (v - lo) / (hi - lo))

    parts: list[str] = []

    # 网格 + 纵轴刻度（4 档）
    for k in range(4):
        v = lo + (hi - lo) * k / 3
        yy = y(v)
        parts.append(
            f'<line class="grid" x1="{L}" y1="{yy:.1f}" x2="{W - R}" y2="{yy:.1f}"/>'
        )
        parts.append(
            f'<text class="axis" x="{L - 10}" y="{yy + 4:.1f}" text-anchor="end">{v:.1f}%</text>'
        )

    pts = [(x(i), y(ys[i])) for i in range(len(hist))]
    poly = " ".join(f"{px:.1f},{py:.1f}" for px, py in pts)
    parts.append(f'<polyline class="line" points="{poly}" fill="none"/>')

    for i, p in enumerate(hist):
        px, py = pts[i]
        label = f"{p['date']}  {p['commit'] or '—'}\n{p['passed']}/{p['total']}  ({p['pct']:.2f}%)"
        parts.append(f'<circle class="point" cx="{px:.1f}" cy="{py:.1f}" r="4.5">')
        parts.append(f"<title>{label}</title>")
        parts.append("</circle>")
        parts.append(
            f'<text class="axis" x="{px:.1f}" y="{H - 14}" text-anchor="middle">{p["date"][5:]}</text>'
        )

    # hover 十字线 + 提示框（JS 驱动）
    parts.append('<line class="crosshair" id="crosshair" y1="0" y2="0" visibility="hidden"/>')
    return "\n".join(parts)


_CSS = """\
:root { color-scheme: light dark; }
.viz-root {
  color-scheme: light;
  --surface: #fcfcfb;
  --page: #f9f9f7;
  --ink: #0b0b0b;
  --ink-2: #52514e;
  --muted: #898781;
  --grid: #e1e0d9;
  --axis: #c3c2b7;
  --series: #2a78d6;
  --series-soft: #cde2fb;
  background: var(--page);
  color: var(--ink);
  font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
  max-width: 860px;
  margin: 0 auto;
  padding: 32px 24px 48px;
}
@media (prefers-color-scheme: dark) {
  .viz-root { color-scheme: dark;
    --surface: #1a1a19; --page: #0d0d0d; --ink: #ffffff; --ink-2: #c3c2b7;
    --muted: #898781; --grid: #2c2c2a; --axis: #383835;
    --series: #3987e5; --series-soft: #184f95;
  }
}
h1 { font-size: 20px; font-weight: 600; margin: 0 0 4px; }
.sub { color: var(--ink-2); font-size: 13px; margin: 0 0 24px; }
.hero { display: flex; align-items: baseline; gap: 12px; margin-bottom: 8px; }
.hero .num { font-size: 48px; font-weight: 650; letter-spacing: -0.02em; }
.hero .unit { color: var(--ink-2); font-size: 14px; }
.delta { font-size: 13px; margin-bottom: 24px; }
.delta .up { color: #006300; }
.delta .down { color: #d03b3b; }
@media (prefers-color-scheme: dark) {
  .delta .up { color: #0ca30c; }
}
svg { width: 100%; height: auto; display: block; }
.line { stroke: var(--series); stroke-width: 2; }
.point { fill: var(--series); stroke: var(--surface); stroke-width: 1.5; }
.grid { stroke: var(--grid); stroke-width: 0.8; }
.axis { fill: var(--muted); font-size: 11px; }
.crosshair { stroke: var(--ink-2); stroke-width: 1; stroke-dasharray: 3 3; }
.tooltip {
  position: absolute; pointer-events: none; background: var(--ink); color: var(--surface);
  padding: 8px 10px; border-radius: 6px; font-size: 12px; line-height: 1.5;
  white-space: pre; transform: translate(-50%, -100%); margin-top: -10px; z-index: 2;
}
table { border-collapse: collapse; margin-top: 32px; width: 100%; font-size: 13px; }
th, td { text-align: right; padding: 7px 12px; border-bottom: 1px solid var(--grid); }
th { color: var(--ink-2); font-weight: 500; }
td:first-child, th:first-child { text-align: left; }
td.num, th.num { font-variant-numeric: tabular-nums; }
tr.latest td { background: var(--series-soft); font-weight: 600; }
a { color: var(--series); }
"""

_JS = """\
const DATA = __DATA__;
const svg = document.getElementById('chart');
const cross = document.getElementById('crosshair');
const tip = document.getElementById('tooltip');
const wrap = document.getElementById('chart-wrap');
const pts = Array.from(svg.querySelectorAll('circle.point'));
const xOf = (el) => parseFloat(el.getAttribute('cx'));
const yOf = (el) => parseFloat(el.getAttribute('cy'));
function move(e) {
  const r = svg.getBoundingClientRect();
  const mx = (e.clientX - r.left) * (svg.viewBox.baseVal.width / r.width);
  const my = (e.clientY - r.top) * (svg.viewBox.baseVal.height / r.height);
  let best = pts[0], bd = Infinity;
  for (const p of pts) { const d = Math.abs(xOf(p) - mx); if (d < bd) { bd = d; best = p; } }
  cross.setAttribute('x1', xOf(best)); cross.setAttribute('x2', xOf(best));
  cross.setAttribute('y1', 24); cross.setAttribute('y2', 340);
  cross.setAttribute('visibility', 'visible');
  const i = pts.indexOf(best);
  tip.textContent = `${DATA[i].date}  ${DATA[i].commit || '—'}\\n${DATA[i].passed}/${DATA[i].total}  (${DATA[i].pct}%)`;
  const wr = wrap.getBoundingClientRect();
  tip.style.left = (e.clientX - wr.left) + 'px';
  tip.style.top = (e.clientY - wr.top) + 'px';
  tip.style.visibility = 'visible';
}
function out() { cross.setAttribute('visibility', 'hidden'); tip.style.visibility = 'hidden'; }
svg.addEventListener('mousemove', move);
svg.addEventListener('mouseleave', out);
"""


def _render_html(data: dict, html_path: Path) -> None:
    hist = data["history"]
    last = hist[-1]
    prev = hist[-2] if len(hist) > 1 else None
    delta = last["pct"] - prev["pct"] if prev else 0.0

    delta_html = ""
    if prev:
        arrow = "▲" if delta >= 0 else "▼"
        cls = "up" if delta >= 0 else "down"
        delta_html = (
            f'<span class="delta">较上次 {arrow} <span class="{cls}">'
            f"{abs(delta):.2f}%</span>（{last['passed']}/{last['total']}）</span>"
        )
    else:
        delta_html = f'<span class="delta">首个数据点（{last["passed"]}/{last["total"]}）</span>'

    rows = []
    for p in reversed(hist):
        cls = ' class="latest"' if p is last else ""
        rows.append(
            f"<tr{cls}><td>{p['date']}</td><td>{p['commit'] or '—'}</td>"
            f'<td class="num">{p["passed"]}/{p["total"]}</td>'
            f'<td class="num">{p["pct"]:.2f}%</td></tr>'
        )
    table = (
        "<table><thead><tr><th>日期</th><th>commit</th><th class='num'>通过</th>"
        "<th class='num'>占比</th></tr></thead><tbody>"
        + "".join(rows)
        + "</tbody></table>"
    )

    data_json = json.dumps(
        [
            {"date": p["date"], "commit": p["commit"], "passed": p["passed"],
             "total": p["total"], "pct": p["pct"]}
            for p in hist
        ],
        ensure_ascii=False,
    )
    js = _JS.replace("__DATA__", data_json)

    svg = (
        f'<div id="chart-wrap" style="position:relative">'
        f'<div class="tooltip" id="tooltip" style="visibility:hidden"></div>'
        f'<svg id="chart" viewBox="0 0 840 380" role="img" '
        f'aria-label="官方题求解率变化曲线">\n{_svg_chart(hist)}\n</svg></div>'
    )

    html = f"""<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>求解能力变化 · SolverOfTheArtisanOfGlimmith</title>
<style>
{_CSS}
</style>
</head>
<body>
<div class="viz-root">
  <h1>官方谜题求解能力变化</h1>
  <p class="sub">{data.get('unit', '')} · 共 {last['total']} 题 · 数据源 docs/solver-history.json</p>
  <div class="hero">
    <span class="num">{last['pct']:.2f}%</span>
    <span class="unit">{last['passed']} / {last['total']} 题解出</span>
  </div>
  {delta_html}
  {svg}
  {table}
  <p class="sub" style="margin-top:24px">曲线纵轴为缩放后的求解率区间（非 0–100），
  以突出小幅变化；完整数值见下表与仓库内 <code>docs/solver-history.json</code>。</p>
</div>
<script>
{js}
</script>
</body>
</html>
"""
    html_path.parent.mkdir(parents=True, exist_ok=True)
    html_path.write_text(html, encoding="utf-8")
    print(f"render: HTML → {html_path}")


def cmd_render(args: argparse.Namespace) -> None:
    data = _load_json(Path(args.json))
    if not data.get("history"):
        raise SystemExit(f"{args.json} 为空，先跑 seed 或 append")
    _render_png(data, Path(args.png))
    _render_html(data, Path(args.html))


# ── main ────────────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description="求解能力历史追踪（CI 趋势图数据管线）")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_seed = sub.add_parser("seed", help="从 official-puzzles-status.md 解析历史点")
    p_seed.add_argument("--status-md", default=str(DEFAULT_MD))
    p_seed.add_argument("--out", default=str(DEFAULT_JSON))
    p_seed.add_argument("--force", action="store_true")
    p_seed.set_defaults(fn=cmd_seed)

    p_append = sub.add_parser("append", help="追加一个 benchmark 数据点")
    p_append.add_argument("--log", required=True, help="benchmark 输出日志")
    p_append.add_argument("--commit", default="", help="本次运行的 git 短 sha")
    p_append.add_argument("--date", default="", help="YYYY-MM-DD（默认今天）")
    p_append.add_argument("--json", default=str(DEFAULT_JSON))
    p_append.set_defaults(fn=cmd_append)

    p_render = sub.add_parser("render", help="生成 PNG + HTML")
    p_render.add_argument("--json", default=str(DEFAULT_JSON))
    p_render.add_argument("--png", default=str(DEFAULT_PNG))
    p_render.add_argument("--html", default=str(DEFAULT_HTML))
    p_render.set_defaults(fn=cmd_render)

    args = parser.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
