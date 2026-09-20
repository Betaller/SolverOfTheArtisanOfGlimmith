# 29 · edge_csp 过度剪枝三连修（bricky_loopy 赌博 / boxy 单洞 / size_sep 合并）

> 状态：**已修复**，2026-09-20。
> 定位：1131/1258 基准里 ring+brick 组合 14 道 FAIL 中多数 attempt 链为
> `edge_csp:exhausted`（甚至 0ms 根层耗尽）——官方解存在但被 edge_csp 剪掉。
> 根因是 `propagate_bricky_loopy` 的“强制前 n 条 Uncut”赌博式推理（§1-4）；
> 同日又修两处同类缺陷：`propagate_boxy_nonboxy` 单洞强制与
> `size_separation_merge_cuts` 合并尺寸检查（§5，boxy / differentiation 簇）。

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

## 5. 同日第二波：boxy / size_separation 的「当前值当最终值」缺陷（已修复）

1148 基准里有 10 道 FAIL 的 attempt 链是 `edge_csp:exhausted`（多数 <100ms）——
搜索空间瞬间"耗尽"但官方解存在。诊断方法与 §1 相同：官方解的全部 Cut 边播种为
`is_boundary` 后 edge_csp 0ms 解出 → 传播器没问题，是根层/probing 的强制有错；
再用 debug 构建的 `Backtrace::force_capture()`（release 下帧被内联吃掉）定位到
具体传播器。根因两处，均为**把「当前尺寸」当「最终尺寸」**推理，且参考 aog
同款（忠实移植带来的）：

### 5.1 `propagate_boxy_nonboxy` 的 non_boxy 单洞强制（area.rs:958）

bbox 只剩 1 洞且 `max_possible >= bbox_size` 时强制 Cut 洞边——但组件可以
**穿过洞继续向外生长、扩大 bbox**，最终仍是非矩形。0497 官方 region 0 =
{(0,0),(0,1),(0,2),(0,3),(1,0),(1,1)} 正是如此：先取 2×2 bbox 的洞 (0,1)，
再右扩到 (0,3)。修复：条件收紧为 `max_possible == bbox_size`（取洞即触顶
封口成矩形 → 真矛盾）。

### 5.2 `size_separation_merge_cuts` 的 merged_sz 检查（area.rs:429）

两组件间 Unknown 边若 `merged_sz == sz1+sz2` 命中任一侧的禁用尺寸集就 Cut——
但 merged_sz 只是合并后的**最小**尺寸，组件还能继续长到合法尺寸。0926 官方
region 14 = {(3,2),(4,2),(4,3),(5,2)}：根层 (3,2)(sz=1,target=4) 与 (4,2)
(sz=1) 的 merged_sz=2 命中密封邻居的尺寸 2 → e=19 被 Cut → 官方解被剪。
修复：仅当 `merged_sz == min(max1, max2)`（合并即触顶，最终尺寸确定为
merged_sz）或**任一侧带 target**（target 钉死最终尺寸，禁用 target 是真冲突）
才 Cut。

### 5.3 收益

新解 8 道 via edge_csp：**0497 / 0688 / 0824 / 0921 / 0926 / 0993 / 1003 /
1091**；0171 / 0932 由 0ms exhausted 转为正常 timeout（空间恢复合法但仍大）。
1200/1032/0699/0850/0848/1378/0341/1370/1340 等 non_block/differentiation
PASS 题抽样 0 回归。

### 5.4 方法论沉淀

1. **播种验证**：把官方解所有 Cut 边写成 `is_boundary` 重跑——0ms 解出说明
   传播器无辜，问题在强制；仍 exhausted 说明传播器直接否定了官方解。
2. **debug 构建 + `Backtrace::force_capture()`**：release 下函数被内联，栈只剩
   `propagate→set_edge`；debug 构建能直接点名传播器（本次靠它 10 分钟锁定
   `propagate_boxy_nonboxy`）。
3. 参考 aog **从不做叶验证**（它假设自己的传播可靠），其 `>=` / 无条件
   `contains(merged_sz)` 这类"当前值当最终值"的推理不能盲信——移植时凡把
   当前尺寸/最小合并尺寸与"最终尺寸约束"比较的，都要问一句"组件还能长吗"。

## 6. 关联

- `rsolver/src/solver/edge_csp/prop.rs::propagate_bricky_loopy`
- `docs/official-puzzles-status.md`（本轮进度行）
- doc 12 §3.2 #3：aog shape cap（OOM 止血，默认 0）
- doc 27 §5.1：同日 exact_piece_count two-piece parity 二次证伪
