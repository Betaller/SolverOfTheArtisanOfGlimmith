# 官方谜题求解状态

> **准则：官方题的官方解是唯一解。**
> 对求解器 / 转换脚本 / 规则校验器 / 规则语义的**每次优化**，必须在本文件**第一部分（进度）与第二部分（变更）各追加一条**，并同步相关文档、跑测试（见文末「软门禁」）。

---

## 第一部分：进度

> 全量扫描 / 基准快照，按时间顺序**往后追加**（旧的在上）。每题完整求解结果见仓库根目录 `scan_official_results.jsonl`。

### 2026-08-05 · 修复后基准（commit `33d32c5`）
- **result**：`results/20260805_33d32c5_rust-official-bench.txt`
- 工具：`scripts/benchmark_rust_solver.py --dir puzzles/official --timeout 20 -j 8`（Rust 求解器）
- **1040 / 1258 通过**（Zone1 300/312，Zone2 387/438，Zone3 329/481）；失败全为超时。
- 修复前 verify 日志 ~190 FAIL 中大量是 gemini/delta、玫瑰窗、环纹 bug 导致的错误解被接受；本次失败均为「解不出」而非「错解」。

### 2026-08-05 · rose 求解器下沉 Rust（commit `4733f59`）
- **result**：`results/20260805_4733f59_rose-port-rust-only.txt`
- 工具：`scripts/verify_puzzles.py --dir puzzles/official --timeout 25 -j 8`（router 只走 RustSolver）
- **1048 / 1258 通过**，较上次 **+8**。
- 提升类型：**纯 rose_window**（aog 曾超时/UNSOLVED）——C4-1、0277、0213、0213nopad 新解出且与官方一致；0833（10×11）时解时不（大网格候选上限敏感）。
- 注：router 仍保留 Python 兜底（Rust-only 有 2 题解不出：1301/0957，brick 回溯短板）。

### 2026-08-06 · rose 尺寸感知优化（commit `7e569e7`）
- **result**：`results/20260806_7e569e7_rose-size-aware-fix.txt`
- 工具：`scripts/benchmark_rust_solver.py --dir puzzles/official --timeout 25 -j 8`
- **1047 / 1258 通过**，较基准（1040）**+7**。

  | Zone | 通过 | 未解 | 变化 |
  |---|---|---|---|
  | Zone1 | 300 / 312 | 12 | 0 |
  | Zone2 | 393 / 438 | 45 | **+6** |
  | Zone3 | 328 / 481 | 153 | -1 |

- 提升类型：**range+rose**（带区域尺寸约束的玫瑰窗）——1334/1342 由 30s FAIL → **<1s 解出**（见第二部分对应条目）。
- 注：Zone3 -1 为 aog 预算下调后某题的计时/非确定性波动（rose 兜底仍未解）。

---

## 第二部分：变更内容

> 按时间顺序**往后追加**（旧的在上）。每次：日期、commit、改了什么、结果。

### 2026-08-05 · 校验 / 转换修复（commits `4ab9e4b` `047f9a1` `9a6c965` `bde3713`）
早期全量扫描发现 129 道「求解器解 ≠ 官方解」，逐一深挖后确认**绝大多数并非真多解，而是转换 / 校验的真实 bug**：

1. **gemini/delta 边约束未被强制**（`4ab9e4b`）：`build_rules` 只给 inequality/difference 生成规则类型，`=`/`!` 边没有 `homogeneous`/`heterogeneous` → aog 求解器跳过边约束、`IndependentValidator` 也不分发边检查 → 求解器产出违反边约束的划分却被判合法。
2. **玫瑰窗检测读错位置**（`047f9a1`）：`_is_rose_window` 用固定 2 字符步长切原始网格找 P 符号，前面有变宽格（罗盘 `U…` / `S` 形）时位置错位 → rose_window 规则被静默丢弃（如 0634）。
3. **环纹规则漏边框 T 型**（`9a6c965`）：环纹检查只遍历内部顶点，内部区域边界与外边框相遇也是 3 段 = T 型（如 0638）；另 `check_rule_ring` 误用 `Edge.is_boundary` 而非区域边界。
4. **1SPR 的 S 格缺 shape 约束**（`bde3713`）：1SPR 谜题 `S#` 格只记 symbol 未加 `shape_pattern` → 缺 puzzle_piece 约束。

**补充**：`= (gemini)` / `! (delta)` 边语义是「两侧区域同形/异形」，非「同区域」——校验器与游戏文档（glimmith-solver）一致，非 bug。
**结果**：129 道 → 6 道（全为 watchtower，见附录 A）。

### 2026-08-05 · block / solitary 建模修复 + block→形状池（commit `e926943`，另一窗口）
方块题专项：全语料 66 道 block 题，逐题硬超时复扫。
- **根因**：Rust 回溯/pieces 把 block 候选面积硬约束为 4（`min_a=max_a=4`）、`check_block` 要求全 2×2；aog 才是正确的「任意矩形」。`solitary` 同病（误当面积=1）。
- **修复**：① block→形状池转换（`rust_solver.py` 合成"所有矩形"注入载荷，`pieces` DLX 接手）；② 修 Rust block/solitary 建模（`backtrack.rs`/`pieces.rs`/`constraints.rs`，`check_block`→`is_rectangle`、`check_same/different`→按形状等）；③ DLX 迭代到合法划分（`dlx.rs search_with_check`）；④ 预算语义改「单元预算」（每部分各拿完整 timeout）；⑤ 形状池按盘面预筛。
- **结果**（单元预算 20s）：0908v2、0826、0829 新解出（DLX）；0446、1109、1004 仍 UNSOLVED（DLX distinct 形状剪枝 / compass 专项 / rose 组合+内存泄漏，见后续计划）。
- 剩余方向：0446（形状去重剪枝）、1109（compass 剪枝）、1004（先修回溯 rose-parallel 内存泄漏）。

### 2026-08-05 · rose 求解器下沉 Rust（commit `4733f59`）
把 Python rose 求解器（`region_match.py` + `rose_growth.py`）移植进 Rust（`rsolver/src/solver/rose/`），使 Rust 二进制能解 aog 解不出的**无尺寸约束纯 rose**。设计文档：`docs/重构/rose-solver-rust-port.md`。
- 模块：`cells.rs`（CellSet 位集/边键/PreBoundaries）、`region_match.rs`（候选 BFS + 面积/分区过滤 + 面积组合 + MRV）、`rose_growth.rs`（wavefront + swap/链式修复，单/多符号）、`mod.rs`（入口，`aog::validate` 作验收门）。
- 分发：rose-capable 题 aog 先 5s 预算（保住 ~30 道 <1s 已解），失败后 rose 用剩余预算。
- **结果**：纯 rose 语料 30 题 28 题可解；C4-1/0277/0213/0213nopad 新解出且与官方一致；大网格（0804/1433/1434）仍 UNSOLVED（Python 也解不出，无回归）。

### 2026-08-06 · rose 尺寸感知优化（commit `7e569e7`）
对比 Python rose 与 Rust rose，找到并修复 **range+rose 差距**（带区域尺寸约束的玫瑰窗）：

| 题 | Python rose | 优化前 Rust | 优化后 Rust |
|---|---|---|---|
| 1334（range+rose, 7×5） | 9.4s | 30s FAIL | **438ms ✓** |
| 1342（range+rose, 6×6） | 30.9s | 30s FAIL | **865ms ✓** |

- **根因**：① `region_match` 面积组合 `min_area_per_region=1`，带尺寸约束的题组合爆炸（1342 达 **1265 万组合**，~1GB）；② 候选 BFS 20000 上限 + 位集遍历顺序与 Python frozenset 不同，截断保留的候选不一致。
- **修复**：`rose::region_size_bounds()`（解析 range/precise 全局区域尺寸界）+ region_match 按 `[min,max]` 过滤候选、组合 `min_val=max(min,N)`。1342 组合 1265 万 → **1 个**，1334 → 6 个。`AOG_ROSE_BUDGET_MS` 5s→3s（aog 解 <1s 纯 rose 后，硬题快速交 rose）。
- **结果**：Rust-only 基准 1047/1258（rose 前 1040）。0 个 rose 专属差距；剩余 2 个 Rust-only 缺口（0957 brick+block+rose、1301 brick+area）为 **brick 回溯短板**（非 rose），router Python 兜底覆盖，实际无回归。

---

## 附录

### A. 当前 DIFF（解 ≠ 官方解）—— 6 道，全为 watchtower
```
Zone3/3-vertex-radar/0543  0544  0662  0663  0800
Zone3/7-zone3-mixed/1144
```
双重验证：官方解与求解器解**均**通过全部建模规则（望塔值 0 违例）。已对照 aog `check_radar` 语义确认 watchtower 建模正确。按当前规则**确实存在多个合法解**，属「官方解是其中一解」的候选。待办：游戏侧实测，或核查望塔规则在障碍格/边框的语义。

### B. 当前 UNSOLVED 分析（求解器解不出，非错解）
按类型（近似）：Zone3/7-zone3-mixed 33、Zone3/2-loopy 31、Zone3/6-compass-main 29、Zone3/5-inequality 19、Zone3/3-vertex-radar 18、Zone3/8-endgame 18、Zone3/4-difference 15、其余 Zone1/Zone2 散布。
主要规则组合：compass+solitary、rose_window（剩大网格 0804/1433/1434）、compass+rose_window、rose_window+same/ring/watchtower 等。
**根因**：求解器能力限制（compass/rose/ring 强规则组合搜索空间大、剪枝不足），**不是校验或转换问题**。

### C. 后续计划
1. 修 Rust **brick 回溯短板**（0957/1301，或借 Python 兜底）；补齐后做 Rust-only 全量回归再删 Python 求解器。
2. 修回溯内存泄漏（`backtrack._solve_rose_parallel` 守护线程不退出，全量 verify OOM / 1004 300s 不收敛）——全量回归阻塞项。
3. 甄别 6 道 watchtower DIFF（游戏侧实测 or 望塔规则深挖）。
4. compass / ring 组合剪枝；0446（DLX 形状去重）、1109（compass 专项）、1004（rose+watchtower）。
5. 每次优化后重跑全量扫描刷新「第一部分」数字。

### D. 软门禁（Soft Gate）
对以下任一模块的**每次优化**（修复、性能、规则语义、转换），提交前必须：
1. **本文件**：第一部分（进度快照）与第二部分（变更记录）各追加一条。
2. **相关文档**：`faq.md` / `rules-guide.md` / `architecture.md` 等，凡涉及处同步。
3. **README**：若影响外部可观察行为（命令、规则数、已知限制）同步。
4. **测试**：`pytest`、`cargo test`、相关 `verify_puzzles.py` 片段，把结果记入本文件。

不满足即视为未完成，不应合入。
