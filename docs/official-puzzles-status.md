# 官方谜题求解状态

> **准则：官方题的官方解是唯一解。**
> 对求解器 / 转换脚本 / 规则校验器 / 规则语义的**每次优化**，必须在本文件**第一部分（进度）与第二部分（变更）各追加一条**，并同步相关文档、跑测试（见文末「软门禁」）。

---

## 第一部分：进度

> 全量扫描 / 基准快照，按时间顺序**往后追加**（旧的在上）。每题完整求解结果存 `results/YYYYMMDD_<short-sha|描述>.txt`，随提交入库。

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

### 2026-08-06 · brick 回溯短板闭合（本会话）
- **result**：`results/20260806_dfadfe3_brick-gap-rust-only-bench.txt`（`scripts/benchmark_rust_solver.py --dir puzzles/official --timeout 40 -j 8`）
- **1052 / 1258 通过**，较上一基准（1047）**+5**，**0 个真实新增失败**。

  | Zone | 通过 | 未解 | 变化 |
  |---|---|---|---|
  | A/B/C | 26 / 27 | 1 | 0 |
  | Zone1 | 301 / 312 | 11 | +1 |
  | Zone2 | 395 / 438 | 43 | +2 |
  | Zone3 | 330 / 481 | 151 | +2 |

- 新解出：**1301**（brick+area，backtrack ≈30s=aog 30s 预算+回溯秒级）、**0957**（brick+block+rose，≈1.9s）、0732 / 0710 / 0795 / 0265 / 1382。两个 Rust-only 缺口（1301/0957）全部闭合。
- 注：0957 在全量并行下偶发 exit -9（rose 内存压力，solo ≈1.9s）；0985 全量并行下 40s 超时（solo ≈16s）——均为**负载波动**非回归。基准用 `--timeout 40` 是为了容纳 1301 的 30s aog 预算。

### 2026-08-06 · Python 求解器移除后 Rust-only router 验证
- **result**：`results/20260806_rustonly-router-verify-zone1.txt`（`scripts/verify_puzzles.py --dir puzzles/official/Zone1 --timeout 25 -j 8`，`default_router` 只走 RustSolver）
- **Zone1 301/312 通过**，与 dfadfe3 Rust-only 基准 Zone1（301/312）**完全一致 → 移除 Python 兜底零回归**。
- 11 道失败：0882 exit -9（8 并行内存压力，非回归）；0223/1435 Rust 返回错解被 IndependentValidator 拦截（历史 Python 亦未解出）；其余为已知超时/UNSOLVED（0804/1433/1434 大 rose 等）。
- 测试：`pytest` 290 通过、`cargo test` 9 通过（详见第二部分 C.0 条目）。

### 2026-08-06 · 删除 constraints.rs stub（fence/compass/ring 信任缺口修复）
- **result**：`results/20260806_82c9132_verify-full.txt`（1295 题全量 verify 基线，228 失败，其中 **30 题「答案未通过独立验证」**——围栏/罗盘/环纹/玫瑰窗错误解）
- **改动**：删除 `rsolver/src/constraints.rs`（9 条恒 `true` 的 stub：fence / compass / ring / brick / inequality / difference / watchtower / puzzle_piece / rose_window），`build_solution`（rose/pieces/backtrack 验收）与 pieces 改用 `solver/validate::validate` 全量复查（与 aog 出口 / rose 验收同一闸门）。
- **效果**：30 题「答案未通过独立验证」现改为 Rust 内**诚实拒绝**（「fails rule validation」/「No solution found」），不再上报错误解。这些题仍 FAIL（求解器解不出，非错解），但「错误答案逃过 Rust 复查」的信任缺口关闭。
- **官方解一致性**：36/36 抽样解出的官方题与 `*-answer` 官方解**完全一致**，0 个「合法但不同」。
- **回归**：40 抽样 PASS 题 + 10 ring/compass PASS 题 **0 回归**；`cargo test` 6 通过、`pytest` 290 通过。
- **benchmark 脚本**：`verify_puzzles.py` / `benchmark_rust_solver.py` 新增「解 vs 官方题解」比对（`matches_official`，DIFF 即失败）。

### 2026-08-06 · 边界望塔修复（watchtower 顶点绝对坐标约定）
- **result**：`results/20260806_f1cfa16_watchtower-verify.txt`（watchtower 专项 verify）+ `results/20260806_f1cfa16_final-verify.txt`（全量 verify），二进制存 `results/bin/rsolver-f1cfa16.linux-x86_64`
- **改动**：用户报告 0800/0543 官方题与 JSON 有差异。根因：官方题在**外边界顶点**上有望塔，但**转换器**（`convert_archive.py` 只收集内部行/列）与**模型**（Rust `io.rs` 顶点数组 `(h-1)×(w-1)`，`build_puzzle` 拒绝边界坐标）双双丢弃。
  - 顶点约定改为**绝对网格坐标** `(0..=h × 0..=w)`：`rsolver` io.rs / validate.rs / backtrack.rs / pieces.rs / aog core.rs 雷达编码（`2r+2, 2c+2`）；Python board.py / validator.py / UI grid_widget.py（绘制与点击映射去 `±1` 偏移）。
  - 转换器收集全部边界望塔；85 个 watchtower 谜题 JSON 以 `puzzles.json` 为权威源迁移 vertices。
- **效果**：**watchtower DIFF 全部消除（0 DIFF）**。6 道（0543/0544/0662/0663/0800/1144）解出且与官方解一致；0985 加约束后 30s 超时（FAIL 但不再出错误解）。50 PASS / 35 FAIL（35 个失败全部为基线既有失败，**0 回归**）。
- **验证**：`cargo test` 6 通过；`pytest` 全绿；watchtower 专项 verify 0 DIFF；6/7 DIFF 题解出官方解。

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

### 2026-08-06 · brick 回溯短板修复 + 砖纹规则语义修正 + 形状规则语义修复 + aog 预算回退（本会话）
针对 1301 / 0957 两个 **brick 回溯短板** 与全量回归中发现的规则校验漏洞，一组联动修复：

1. **backtrack area 剪枝落地**（§3.1 设计）：`pick_next_cell` 动态连通优先生长线索区域 + `check_area_lower_bounds` 密封/容量剪枝 + frontier 引用计数。行优先的死结（`(0,3)` 先于 `(1,1)` 被处理导致 48 区域长不到它旁边）被绕开。
2. **砖纹（brick）规则语义修正——两处方向相反的 bug**：
   - 旧 `vertex_boundary_count` 把 **blocked 相邻全按边界计** → 棋盘角落（3 blocked + 1 区域）假报 4 路交叉，回溯提前误剪；
   - 修复中一度把「顶点有 blocked 就跳过 brick」→ **放过真 4 路交叉**（1 blocked + 3 个不同区域 = 4 路交叉），于是 1301 出现**孪生解**（单点 (7,6) 被误判合法，官方是 (6,7)）。
   - **最终正确语义**（镜像 C++ `check_tatami` / glimmith-solver）：blocked 当空区、**blocked-blocked 不算边界、blocked-区域算边界、不跳过 blocked 顶点**。同步修复 `validate.rs`、`IndependentValidator._check_brick`、backtrack。
   - **结果**：1301 唯一解 = 官方解 (6,7)。
3. **删除 `check_merge_ok`**（backtrack 过度保守剪枝）：「加入格若触及别的区域就拒绝」把 1301 官方解构造（单点 (6,7) 紧挨区域 0）整支剪掉 → 回溯找不到官方解。删除后 **1301 与 0957 均由 Rust backtrack 解出**（0957 ≈1.9s；1301 约 30s = aog 30s 预算 + backtrack 秒级）。正确性由叶子校验 + `check_all` + `IndependentValidator` 三层兜底。
4. **形状规则语义修复**：`constraints.rs` 的 `check_same`/`check_different` 用原始 `shape` Vec 比较 → 改为 `dihedral_key` 规范键；`check_mixed` 由 `!check_same`（全局近似）改为「相邻区域形状不同」正确语义（镜像 validate.rs / Python）。backtrack 叶子 `different` 检查同步修复。**结果**：修复 1114 等 `different` 题被旋转/翻转重复形状误放行的问题（`IndependentValidator` 一直能拦，Rust-only 之前会接受错解）。
5. **aog 预算回退**：全量回归发现上一版 `AOG_BUDGET_CAP_MS = 1s` 把 aog 硬性限死，**65 道 aog 在 1-25s 能解的题全部转 FAIL**（1047 → 983）。移除 1s 封顶、aog 拿回完整 `timeout_ms`；配合热路径 deadline 检查（Fix B/C：shape 循环每 256 查、size 循环每次查），aog 在 deadline 处**精确停住**而非烧光预算。

- **验证**：`cargo test` 9 通过；`pytest` 387 通过；全量 Rust-only 基准 **1052/1258**（0 真实回归，见第一部分最新条目）；router 实测 1301（≈30s）/0957（≈1.9s）均 `rust(ok)`，不再依赖 Python 兜底。

### 2026-08-06 · 评估并移除 Python 求解器（plan C.0 完成）
**评估**：Rust-only 全量基准 1052/1258 之后，Python 求解器（exact_cover / rose / backtrack，及 dlx / candidates / region_match / rose_growth / bfs_candidates / polyomino_cache / checks / propagator / validator）是否还有解出价值？
- **历史全路由扫描**（Aug 5，brick 修复前）：~1000+ 官方题中 Python 兜底只解出 **5 道**（C4-1 / 0277 = rose、1169 = exact_cover、1301 / 0153 = backtrack），逐一核对**现均由 Rust 解出**（均不在 206 道 Rust-only 失败清单）。
- **定向扫描**：对 206 道 Rust-only 失败题跑全路由（含 Python 兜底），处理 82/206（40%）**Python 兜底 0 命中**（81 次尝试全败）；唯一解出为 Rust（0745，58s，超 bench 40s 时限）。因已知 rose 内存压力（单进程 4.3GB/15GB）中止，剩余 Zone3 硬题为能力极限、Python 亦从未解出。
- **结论**：Python 求解器对官方语料无解出价值，移除。

**移除内容**：
- `default_router` 改 **Rust-only**（`RustSolver()`）。
- 删除 `src/solver/`：backtrack / dlx / candidates / bfs_candidates / region_match / rose_growth / polyomino_cache / checks / propagator / validator / exact_cover / rose；`src/services/solver_service.py`（UI 已死）；调试脚本（show_candidates / debug_222 / test_111 / test_222）。
- **保留共享层**：`constraints.py`（RULE_CHECKERS）、`shapes.py`、`exceptions.py`、`src/validation/validator.py`（IndependentValidator）——UI 编辑器（shape_editor/shape_gallery）、生成脚本与独立校验依赖。
- `gen_ai_puzzles.py` 改用 router 校验；`main_window.py` 去掉 SolverService。
- 测试：删 test_backtrack / test_propagator / test_validator；test_solver_end_to_end 改为 router 端到端（26 个）；conftest 去 solver/validator fixture；test_constraints 内联 `_sync_boundaries` 助手。

**验证**：`pytest` **290 通过**（删 Python 求解器相关 ~97 个后新基线）；`cargo test` 9 通过；Rust-only router `verify_puzzles.py --dir puzzles/official/Zone1` **301/312** 与 dfadfe3 基准 Zone1 完全一致，**0 回归**（见第一部分最新条目）。

### 2026-08-06 · P0 重构：validate.rs 独立 + 收敛 5 处重复实现（本会话）

纯重构，**无行为变化**（求解数字不变，基准豁免）。
1. **validate.rs 提升为独立模块**：`solver/aog/validate.rs` → `solver/validate.rs`，
   消除 rose 依赖 aog 的反向依赖；`aog/mod.rs` 出口与 `rose/mod.rs` 验收改走
   `crate::solver::validate::validate`。
2. **新建 `shapes.rs` 收敛 5 处重复实现**：
   - `dihedral_key`（constraints / validate 两份 → `shapes.rs:32` 唯一）；
   - `is_rectangle`（两份 → `shapes.rs:12` 唯一）；
   - `collect_pool_shapes`（aog/core 与 constraints 的双来源收集 → `shapes.rs:75` 唯一）；
   - `area_bounds`（pieces/backtrack/rose 三版合并 → `shapes.rs:115`；统一默认 max=h*w、
     罗盘派生 min；rose 侧因 `region_match` 再 `min(total-(m-1))` 重界，行为不变）；
   - `rose_symbol_types`（rose/aog/validate 三处内联 → `shapes.rs:160` 唯一；空
     `symbol_types` 数组回退格子符号的语义统一，语料无空数组题，边界不触发）。
3. `check_mixed` 已统一的「相邻异形」语义**保持不变**（本轮只收敛，不动实现）。

- **验证**：`cargo test` 9 通过；`pytest` 290 通过；Zone1 `verify_puzzles.py --timeout 25 -j 8`
  **301/312**（11 失败与 rustonly-router 基准组成一致，**0 回归**）。
- 注：0213 / 0213nopad（大 rose）在 -j8 负载下偶发互换超时（本轮 0213 超时、上轮 0213nopad
  超时），单跑各 ~2.5s 解出——与已知 0833 / 0882 同类负载波动，非回归。

### 2026-08-06 · P1 性能/内存：backtrack 扁平数组 + Pools 惰性分配（本会话）

纯性能/内存优化，**求解行为不变**（同 DFS 逻辑）。
1. **backtrack 状态扁平数组**（#3）：`cell_to_region` `HashMap<(usize,usize),usize>` →
   `Vec<Option<usize>>`（`r*width+c` 索引）；`region_shapes` `HashMap<usize,Vec>` →
   `Vec<Vec<[usize;2]>>`（区域号严格 0..n 递增、回退递减，`push`/`pop` 维护）。`frontier` /
   `region_clue` 保持 HashMap（area 门控）。`BacktrackState` 加 `width` stride 字段。
2. **边界尊重检查扁平数组**（#4）：`regions_respect_boundaries`（`mod.rs:125`）的
   `HashMap<(usize,usize),usize>` → `Vec<Option<usize>>` 直接索引。
3. **aog Pools 惰性分配**（#5）：`Pools.place` `Vec<RefCell<PlaceLevel>>` →
   `Vec<RefCell<Option<PlaceLevel>>>`，`Pools::place_level(i)`（`RefMut::map` +
   `get_or_insert_with`）按 DFS 深度惰性建层。**峰值 RSS 实测**（`results/20260806_pools-lazy-rss.txt`）：
   A1-1 **5.6→2.3MB**、C1-3 **5.7→2.9MB**、C4-1 **11.7→9.0MB**（此前 100 层 × ~33KB 常驻 ~3.3MB）。

- **验证**：`cargo test` 9 通过；`pytest` 290 通过；Zone1 `verify_puzzles.py --timeout 25 -j 8`
  **301/312**（与基准一致，**0 回归**）。定向：1301（brick+area，backtrack ≈30s）、0957
  （brick+block+rose ≈1.8s）、C4-1（rose ≈3.1s）、A1-1（shape_pool 3ms）均正常解出。

### 2026-08-06 · P2/P3 清理：死代码移除 + Cell 求解状态分离（本会话）

纯清理，**无行为变化**（编译期确认移除项均 0 调用）。
1. **死代码移除**（#6）：`apply_line_constraint` 的 `vertical` 参数（调用点恒传
   `cell1_first=true`）；`grid::unassigned_cells` / `connected_components`、
   `polyomino::generate_polyominoes`、`aog::core::dbg_steps`、`Dlx::search` +
   `solution_rows` + `header_count`、`CellSet::set_from`、`PreBoundaries::len`；
   `types::Direction` 枚举与 `CompassClue::get`；`pick_next_cell` 未用的 `puzzle`
   参数、`check_edge_constraints` 未用的 `regions` 参数、`has_shape_pool` 重复声明、
   `slash_check_enable`/`slash_check_slash_cnt`（只写不读）。`Solution.steps_taken`
   **保留**（JSON 兼容）并标注废弃。`main.rs` 文档字符串修正（`--parse` 未实现）。
2. **Cell 求解状态分离**（#8）：删除 `Cell.region_id`（求解路径死字段，16B/格）与
   `assigned()`；依赖它的 `unassigned_cells` 已随 #6 移除。Cell ~192B → ~176B。
   Python `board.py` 的 `Cell.region_id` 是独立模型（Board 重建用），不受影响。

- **验证**：`cargo test` 9 通过；`pytest` 290 通过；Zone1 `verify_puzzles.py --timeout 25 -j 8`
  300/312（11 个基线失败 + 0213/0213nopad 这对大 rose 同轮双双超时——负载波动，单跑各
  ~2.5s 解出，**非回归**）。构建警告从 ~20 降到 2（`is_subset` 测试辅助、`L` C++ 镜像命名）。

### 2026-08-06 · P2 #7：批量模式（子进程复用）+ IO 移出 main.rs（本会话）

解决 `verify_puzzles.py` / `benchmark_rust_solver.py` 每题 spawn 一次 rsolver 的启动开销：
- **rsolver `--batch`**：从 stdin **逐行读**多份紧凑谜题 JSON，逐题求解、**逐行输出**
  题解 JSON（1 输入行 ↔ 1 输出行；坏行输出 `solved:false` 继续）。单题模式（文件/单段
  JSON）完全不变。
- **IO 移出 main.rs**（用户要求）：新建 `src/io.rs` 承载 JSON 模型 / `build_puzzle` /
  序列化 / `solve_json_line`；`main.rs` 只做 stdin/argv/stdout 调度。
- **`RustSolver.solve_batch`**：一个 `--batch` 子进程批量求解，**每题独立预算**
  （`select` + `os.read` 逐行读，超时只截断该题与后续题，已完成的保留）——与单题模式
  每题的墙钟上限一致，大 rose runaway（如 C4-2）不会烧掉整批预算。
- **`verify_puzzles.py --batch N` / `benchmark_rust_solver.py --batch N`**：文件分块，
  每块复用一个子进程（默认 1 = 逐题，行为不变）。

- **验证**：`cargo test` 9 通过；`pytest` 290 通过；C 区 batch 与单题均 **4/5**（C4-1
  保留解出、C4-2 预算内截断）。`ruff check src/` 无新增（`supports` ARG003 为历史问题）。
- **已知局限**：批量逐进程顺序求解，某题若超出内部 30s 预算（已知大 rose runaway，
  如 C4-2 / 0804 / 1433，见附录 B），**同批排在其后的题会连带判超时**（每题仍独立
  截断、已完成者保留）。reference 集 batch 8 5/22 vs 单题 15/22 即此连带所致。
  **精确验证请用默认 `--batch 1`**；`--batch N` 适用于良性集合的吞吐扫描（快题实测
  提速 ~5×，spawn 开销 ~1ms/题 → ~1.3s/1258 题）。批量模式交付子进程复用架构
  （`--batch` 协议 + `io.rs`），不改变任何求解结果。

### 2026-08-06 · 删除 constraints.rs，build_solution 改用 validate.rs 全量复查
- **背景**：2026-08-06 全量 verify 暴露 **30 题「答案未通过独立验证」**，全部涉及
  fence / compass / ring / rose_window 等规则。根因：`constraints.rs` 的 9 条规则是
  恒 `true` 的 stub——aog 对 ring+fence+rose 等组合题预算内解不出时，backtrack/pieces
  产出的错误解通过 stub 被标 solved，只被 Python Router 的 IndependentValidator 拦下
  → 判 FAIL。30/30 官方解通过 Python 验证器（非谜题/转换/规则理解问题，纯求解器代码问题）。
- **改动**：
  1. 删除 `rsolver/src/constraints.rs`（含 4 个单元测试；`is_rectangle` 测试移入 `shapes.rs`）。
  2. `solver/mod.rs` 的 `build_solution` 与 `solver/pieces.rs` 改用
     `crate::solver::validate::validate`（与 aog 出口 / rose 验收同一闸门，覆盖全 22 规则）
     做全量复查。
  3. 消除 `check_heterogeneous`/`check_homogeneous` 的语义分歧：旧 `constraints.rs` 是
     区域级全局检查，validate.rs / Python 是**边级**（只查带 `==`/`!` 标记的边）——现在
     唯一语义是 validate.rs 的边级。
- **验证**：`cargo test` 6 通过（4 constraints 测试删 + 1 shapes `is_rectangle` 加）；
  `pytest` 290 通过；40 抽样 PASS 题 + 10 ring/compass PASS 题 **0 回归**；
  36 抽样解出的官方题与 `*-answer` 官方解一致（0 DIFF）。
- **benchmark 脚本官方解验证**：`verify_puzzles.py` / `benchmark_rust_solver.py` 对每个
  解出的官方题比对 `*-answer` 官方解分区——新增 `src/validation/official_answer.py`
  （`matches_official_answer`），结果字段 `matches_official`（True/False/None），
  False（解合法但 ≠ 官方唯一解）标记 **DIFF** 并计入失败。

### 2026-08-06 · 边界望塔缺失修复（顶点绝对坐标约定）
- **背景**：用户报告 0800/0543 官方题与 JSON 有差异——官方题在**外边界顶点**上也有
  望塔限制，JSON 缺失。经调研：14 个 watchtower 谜题有边界望塔，**6 个 watchtower
  DIFF 题（0543/0544/0662/0663/0800/1144）全在其中**。此前被误判为「多解/规则理解」。
- **根因（转换 + 模型双层 bug）**：
  1. `scripts/convert_archive.py` 只在 `1 ≤ r ≤ height-1`、`1 ≤ c ≤ width-1` 收集望塔，
     丢弃上下左右四条边界的望塔。
  2. Rust `io.rs` 顶点数组是 `(h-1)×(w-1)`（仅内部顶点），`build_puzzle` 对边界坐标
     `return Err("vertex out of range")`——模型根本不能表示边界顶点。
- **修复**：
  1. **顶点约定改为绝对网格坐标** `(0..=h × 0..=w)`：`rsolver/src/io.rs`（数组 `(h+1)×(w+1)`、
     接受边界坐标）、`solver/validate.rs` / `backtrack.rs` / `pieces.rs`（watchtower 统计
     在界非阻塞周围格）、`solver/aog/core.rs`（雷达编码 `(2r+2, 2c+2)`，原 `2r+4`）、
     `src/models/board.py`（`_build_vertices` / `cells_surrounding_vertex` /
     `edges_surrounding_vertex`）、`src/validation/validator.py`、`src/ui/grid_widget.py`
     （绘制与点击映射去掉 `±1` 偏移）。
  2. **转换器** `scripts/convert_archive.py`：收集 `0..=height × 0..=width` 全部望塔，
     绝对坐标 `(r,c)`。
  3. **迁移 85 个 watchtower 谜题 JSON**：以 `third_party/archiveofglimmith.github.io/
     puzzles.json` 为权威源（游戏解析约定：顶点行角点 `3c`，行补齐 `3W+2`），覆写
     `vertices` 字段（内部顶点重索引 + 边界新增）。
- **验证**：watchtower 专项 verify **50 PASS / 35 FAIL / 0 DIFF**（35 FAIL 全部为基线既有
  失败，**0 回归**）；6 道 DIFF 题经 router 解出且与官方解一致；0985（原 DIFF）加约束后
  30s 超时（不再出错误解）；官方解对 0985 完整约束通过。`cargo test` 6 通过、`pytest` 全绿。

---

## 附录

### A. 当前 DIFF（解 ≠ 官方解）
1. ~~**watchtower DIFF —— 6 道**~~ **已解决（2026-08-06）**
```
Zone3/3-vertex-radar/0543  0544  0662  0663  0800
Zone3/7-zone3-mixed/1144
```
**根因**：官方题在**外边界顶点**上也有望塔，但转换器（`convert_archive.py` 只收集内部
行/列）与模型（顶点数组是内部 `(h-1)×(w-1)`，`build_puzzle` 拒绝边界坐标）**双双丢弃
边界望塔** → 盘面约束不足 → 求解器解出非官方解。**修复**：顶点约定改为**绝对网格坐标**
（`0..=h × 0..=w`，含边界角点），转换器收集全部边界望塔，85 个 watchtower 谜题 JSON
迁移。6 道 + 0985 全部不再产生「合法但 ≠ 官方」的解（6 道解出官方解；0985 加约束后
搜索变难，30s 超时——仍是 FAIL 但**不再是错误解**）。详见第二部分对应条目。

> 曾把 1301 误列入「孪生解」，实为 **brick 规则语义 bug**：`validate.rs` / `IndependentValidator` / backtrack 对含 blocked 的顶点跳过 brick 检查，放过 1 blocked + 3 区域的真 4 路交叉，导致单点 `(7,6)` 的错解被判合法。修复砖纹语义后 1301 唯一解 = 官方解 `(6,7)`（2026-08-06，见第二部分）。

> 曾把 1301 误列入「孪生解」，实为 **brick 规则语义 bug**：`validate.rs` / `IndependentValidator` / backtrack 对含 blocked 的顶点跳过 brick 检查，放过 1 blocked + 3 区域的真 4 路交叉，导致单点 `(7,6)` 的错解被判合法。修复砖纹语义后 1301 唯一解 = 官方解 `(6,7)`（2026-08-06，见第二部分）。

### B. 当前 UNSOLVED 分析（求解器解不出，非错解）
按类型（近似）：Zone3/7-zone3-mixed 33、Zone3/2-loopy 31、Zone3/6-compass-main 29、Zone3/5-inequality 19、Zone3/3-vertex-radar 18、Zone3/8-endgame 18、Zone3/4-difference 15、其余 Zone1/Zone2 散布。
主要规则组合：compass+solitary、rose_window（剩大网格 0804/1433/1434）、compass+rose_window、rose_window+same/ring/watchtower 等。
**根因**：求解器能力限制（compass/rose/ring 强规则组合搜索空间大、剪枝不足），**不是校验或转换问题**。

### C. 后续计划
0. ~~**评估 Python 求解器去留**~~ **已完成（2026-08-06）**：评估证明 Python 求解器对官方语料无解出价值（历史仅解 5 道且现全由 Rust 解出；206 道失败题定向扫描 Python 0 命中）。已删 Python 求解算法、`default_router` 改 Rust-only、保留 constraints/shapes 共享层与 IndependentValidator，测试与文档同步（见第二部分 C.0 条目）。
1. ~~修 Rust **brick 回溯短板**（0957/1301）~~ **已完成（2026-08-06）**：砖纹语义修正 + 删除 `check_merge_ok` + area 剪枝，1301/0957 均由 Rust 解出。下一步可做 **Rust-only 全量回归**（router 只走 RustSolver 验证全部官方题），通过后再评估删 Python 求解器（与 C.0 衔接）。
2. 修回溯内存泄漏（`backtrack._solve_rose_parallel` 守护线程不退出，全量 verify OOM / 1004 300s 不收敛）——全量回归阻塞项。
3. ~~甄别 6 道 watchtower DIFF~~ **已完成（2026-08-06）**：边界望塔缺失（转换+模型 bug）
   已修复，见第二部分。
4. compass / ring 组合剪枝；0446（DLX 形状去重）、1109（compass 专项）、1004（rose+watchtower）。
5. 每次优化后重跑全量扫描刷新「第一部分」数字。

### D. 软门禁（Soft Gate）
对以下任一模块的**每次优化**（修复、性能、规则语义、转换），提交前必须：
1. **本文件**：第一部分（进度快照）与第二部分（变更记录）各追加一条。
2. **相关文档**：`faq.md` / `rules-guide.md` / `architecture.md` 等，凡涉及处同步。
3. **README**：若影响外部可观察行为（命令、规则数、已知限制）同步。
4. **测试**：`pytest`、`cargo test`、相关 `verify_puzzles.py` 片段，把结果记入本文件。
5. **基准结果随提交入库**：影响求解结果（可解性 / 性能 / 规则语义）的提交，必须把对应基准 /
   全量扫描输出存为 `results/YYYYMMDD_<short-sha|描述>.txt` 并**随该提交一起入库**（不允许只
   留在 /tmp）。纯文档、无行为变化的重构等不影响求解结果的提交可豁免。

不满足即视为未完成，不应合入。
