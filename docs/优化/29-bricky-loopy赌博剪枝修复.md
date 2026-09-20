# 29 · bricky_loopy 赌博式剪枝修复（ring+brick 簇根因）

> 状态：**已修复**，2026-09-20。
> 定位：1131/1258 基准里 ring+brick 组合 14 道 FAIL 中多数 attempt 链为
> `edge_csp:exhausted`（甚至 0ms 根层耗尽）——官方解存在但被 edge_csp 剪掉。
> 根因是 `propagate_bricky_loopy` 的“强制前 n 条 Uncut”赌博式推理。

---

## 0. 一句话结论

顶点度约束超限时，代码只知道「n 条 Unknown 里**至少** k 条最终 Uncut」，
却把它实现成「**前 k 条** Uncut」——挑哪几条是赌博。1378 上根层（nodes=0）
就把 cell (2,2) 的 W/E 两条边错误强成 Uncut，palisade 传播器随后按
fence_pattern=Three（需 3 条 Cut）判定矛盾，官方解被剪掉。

## 1. 定位过程

1. `EDGE_CSP_DEBUG=1` 跑 1378：`edge_csp: nodes=0 ... exhausted`（0ms 根层死）。
2. 给 `propagate()` 加临时 `step!` 宏标注各传播器的 Err 来源：
   `ERR from palisade nodes=0 unknown=40`。
3. palisade 拒绝点打印：`cell=(2,2) states=[Unknown,Unknown,Uncut,Uncut] kind=Three`。
4. 对照官方解：cell (2,2) 的 W/E 都是 **Cut**（region 1 vs region 0）。
   根层传播却已定成 Uncut → 别的传播器误强。
5. 读 `propagate_bricky_loopy`：1378 是 `brick+ring`，走
   `cut_count + unk > 2 → 强制前 (cut-2) 条 Uncut` 分支——即赌博剪枝。

## 2. 修复

只在**全部** Unknown 都必须 Uncut 时才逐边强制（可靠情形）：

| 分支 | 修复前 | 修复后 |
|---|---|---|
| ring+brick | `cut+unk > 2` → 前 `cut+unk-2` 条 Uncut | 仅 `cut_count == 2` → 全部 unk Uncut；`cut >= 3` → 矛盾 |
| bricky-only | `cut+unk > 3` → 前 `cut+unk-3` 条 Uncut | 仅 `cut_count == 3` → 全部 unk Uncut；`cut > 3` → 矛盾 |

`cut_count` 更低而总数超限的情形（如 0 cut + 4 unk、上限 2）：只知道「4 条里
至少 2 条 Uncut」，**不挑边**，留给搜索与 probing。

loopy-only 分支（3+1→Cut、2+1→Uncut、3+0→矛盾）本就是全选/矛盾情形，未改动。

## 3. 收益（2026-09-20 全量，`--timeout 40 -j 6`，commit `61df73a`）

基准：`results/bench/20260920_61df73a_bricksound-oomretry.{txt,jsonl}` →
**1146 / 1258**（基线 1130，+16）。

- 新解 9 道（全部 via edge_csp）：**1373 / 1374b / 1375 / 1378 / 0834 / 0631 /
  1110 / 0977 / 0978**，另有 0209 / 0418 / 0491 / 0630 / 0952 / 0969 / 1294 /
  1301 等历史 FAIL 噪声带内翻正。
- 这 9 道中 1373/1375/1378/0834/0977/0978 在 -j 6 下被 aog OOM（exit -9）拖死。
  为此加了 **OOM 兜底重试**（`rust_solver.py`）：exit -9 时以
  `AOG_SHAPE_CAP=200000` 原预算重跑一次，aog 退化为优雅超时、edge_csp 接手。
  默认路径 cap 保持 0——实测 0710（area 8×8）合法形状库超 10M 条目，任何全局
  cap（50k/200k/2M/10M 全试过）都会回归它；0384/0870/1131 在 200k 下亦回归、
  2M 下恢复。对照实验（无兜底）：`results/tmp/20260920_bricksound.jsonl` =
  1137/1258。
- 0745 本轮 -j 6 下翻负，单跑 SOLVED → 噪声，非回归。
- 无规则语义变化；validator 不涉及；声音性**修复**（从不可靠变可靠）。

## 4. 教训

传播器里凡「至少 n 个中选 k 个」的约束：

- `k == n`（全选）或 `k == 0`（全不选）→ 可以落地为逐边强制/矛盾；
- `0 < k < n` → **不能**挑边，否则是赌博；要么留给搜索，要么上 dedicated
  nogood / probing（probing 是逐值试探后强制另一值，是可靠的）。

验证手段：官方解是唯一解，取官方解在根层传播后比对每条边状态——任何与官方解
冲突的根层强制都是声音性 bug。本次即靠这个方法 10 分钟锁定 palisade 误判上游。

## 5. 关联

- `rsolver/src/solver/edge_csp/prop.rs::propagate_bricky_loopy`
- `docs/official-puzzles-status.md`（本轮进度行）
- doc 12 §3.2 #3：aog shape cap（OOM 止血，默认 0）
- doc 27 §5.1：同日 exact_piece_count two-piece parity 二次证伪
