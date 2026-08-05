# 官方谜题求解状态与结论

> 状态：持续维护。**每次对求解器 / 转换脚本 / 规则校验器 / 规则语义的任何优化，都必须更新本文件**（见文末「软门禁」）。

## 1. 结论（2026-08-05 深挖）

**准则：官方题的官方解是唯一解。**

早期全量扫描发现 129 道「求解器解 ≠ 官方解」，逐一深挖后确认**绝大多数并非真多解，而是转换 / 校验的真实 bug**。已修复：

| # | Bug | 根因 | 修复 commit |
|---|---|---|---|
| 1 | gemini/delta 边约束未被强制 | `build_rules` 只给 inequality/difference 生成规则类型，`=`/`!` 边没有 `homogeneous`/`heterogeneous` → aog 求解器跳过边约束、`IndependentValidator` 也不分发边检查 → 求解器产出违反边约束的划分却被判合法 | `4ab9e4b` |
| 2 | 玫瑰窗检测读错位置 | `_is_rose_window` 用固定 2 字符步长切原始网格找 P 符号，前面有变宽格（罗盘 `U…` / `S` 形）时位置错位 → rose_window 规则被静默丢弃（如 0634） | `047f9a1` |
| 3 | 环纹规则漏边框 T 型 | 环纹检查只遍历**内部顶点**（`board.vertices()`），而内部区域边界与**外边框**相遇也是 3 段 = T 型（如 0638 的 L 形划分）。另 `check_rule_ring` 误用 `Edge.is_boundary`（预画边界）而非区域边界 | `9a6c965` |
| 4 | 1SPR 的 S 格缺 shape 约束 | 1SPR 谜题中 `S#` 格只记 symbol，未加 `shape_pattern` → 缺少 puzzle_piece 约束 | 用户修复（未提交） |

**补充发现**：`= (gemini)` / `! (delta)` 边在游戏中的语义是「两侧区域**同形/异形**」，不是「同区域」——项目校验器与游戏文档（glimmith-solver rules-inventory）一致，非 bug。

## 2. 解题进度（全量扫描，2026-08-05）

扫描方式：每题经 `default_router()`（Rust aog→pieces→backtrack 链）求解，用 `IndependentValidator` 独立校验，并与官方 answer 文件对照。`scripts/verify_puzzles.py` 现支持 `--rules <type>`（按规则过滤）与 `--out <file>`（JSONL 存档，schema 对齐 `scan_official_results.jsonl`），可按规则做专项扫描。

| 状态 | 数量 | 占比 | 说明 |
|---|---|---|---|
| MATCH（与官方一致） | 982 | 79.8% | |
| DIFF（解≠官方） | 6 | 0.5% | 全为 watchtower，见 §3 |
| UNSOLVED（求解器解不出） | 235 | 19.1% | 见 §4 |
| 无官方解（0067 / 1130） | 2 | — | archive 无 solution 字段，无 answer 文件 |
| 未跑完（超难，卡在回溯） | 7 | — | 回溯候选枚举超时，见 §5 |

> 数字为 2026-08-05 修复后全量扫描（1224/1231 已完成）。每题的完整求解结果见仓库根目录 `scan_official_results.jsonl`。

### 2.1 基准（benchmark，2026-08-05 修复后）

| 工具 | 范围 | 结果 |
|---|---|---|
| `scripts/benchmark_rust_solver.py`（Rust 求解器） | puzzles/official 1258 题 | **1040 / 1258 通过**（Zone1 300/312, Zone2 387/438, Zone3 329/481），失败全为 20s 超时 |
| `scripts/verify_puzzles.py`（完整路由链） | puzzles/official | 跑到 990/1258 被 OOM 中断（862 PASS / 128 FAIL）——回溯 rose-parallel 线程内存泄漏，见 §5 |

**失败均为 UNSOLVED（求解器超时/无解），不再有「接受了非法解」的失败。** 修复前 verify 日志里有 ~190 FAIL，其中大量是 gemini/delta、玫瑰窗、环纹 bug 导致的错误解被接受。

完整原始输出：`results/benchmark_rust_official_33d32c5.txt`（头部标注脚本、commit `33d32c5`、代码状态 `bde3713`+`9a6c965`）。

另：`scan_official_results.jsonl`（仓库根目录）保存了每题的求解结果（区域划分、耗时、规则、校验结果），可复用于后续对比。

## 3. 解 ≠ 官方解（DIFF）分析

**仅剩 6 道，全为 watchtower 规则**：

```
Zone3/3-vertex-radar/0543  0544  0662  0663  0800
Zone3/7-zone3-mixed/1144
```

每道都做了双重验证：官方解与求解器解**均**通过全部建模规则（望塔值逐一核对，0 违例）。已对照 aog `check_radar` 语义确认 watchtower 规则建模正确。因此这 6 道在 archive 编码的规则下**确实存在多个合法解**，属「官方解是其中一解，而非唯一解」的候选。

> 待办：若坚持官方解必唯一，需在游戏侧实测这 6 道（或核查望塔规则在障碍格/边框附近的语义是否还有遗漏）。

## 4. 未解（UNSOLVED）分析

按类型分布：

| 类型 | 未解数 | 主要规则 |
|---|---|---|
| Zone3/7-zone3-mixed | 33 | 混合（compass/rose/ring 组合） |
| Zone3/2-loopy | 31 | ring（环纹） |
| Zone3/6-compass-main | 29 | compass |
| Zone3/5-inequality | 19 | inequality |
| Zone3/3-vertex-radar | 18 | watchtower |
| Zone3/8-endgame | 18 | 混合 |
| Zone3/4-difference | 15 | difference |
| 其余 Zone1/Zone2 | 72 | 散布 |

按规则组合（前几名）：

| 规则组合 | 未解数 | 原因推测 |
|---|---|---|
| compass + solitary | 13 | compass 搜索空间大 |
| rose_window（纯） | 6 | 玫瑰窗区域匹配 |
| inequality | 6 | 面积不等约束难传播 |
| compass + rose_window | 6 | 双强规则组合 |
| rose_window + same / ring / watchtower | 13 | 组合剪枝不足 |

**根因**：主要是**求解器能力限制**——compass、rose_window、ring 等强规则的组合搜索空间大，现有剪枝不足，在超时内找不到解。**不是校验或转换问题**（这些题不再产出错误答案，只解不出）。

## 4.1 方块规则（block）题专项（2026-08-05）

全语料含 `block`（方块）规则的题共 **66 道**（official 65 + reference 1，user/aiGen 无）。逐题硬超时复扫（`scripts/verify_puzzles.py --rules block --out <file>`，单元预算 20s/部分），当前状态：

| 状态 | 数量 | 说明 |
|---|---|---|
| 求解器可解（校验通过） | 63 | 含 0835 / 0836 两题求解器解 ≠ 官方解（官方解非唯一，与 §3 同类） |
| 求解器解不出 | 3 | 0446 / 1109（各部分 20s 预算内失败）+ 1004（rose 组合病理超时） |
| **真无解** | **0** | 3 道解不出的题，官方解经 `IndependentValidator` 全规则校验**全部通过** |

剩余 3 道：different+block（0446，14×14）、block+area+compass+solitary（1109，12×12）、block+watchtower+rose_window（1004，13×13）。

根因（两条，均有代码实证）：
1. **Rust 回溯 / pieces 对 `block` 的建模错误**：`backtrack.rs:80-83`、`pieces.rs:138` 把 block 的候选区域面积硬约束为 4（`min_a=max_a=4`），`constraints.rs:204-207` 的 `check_block` 更要求所有区域 == 2×2 方块；而 aog（`core.rs:614` `only_rectangles`、`validate.rs:164` `is_rectangle`）才是正确的"任意矩形"。回溯用 `max_area` 硬性截断区域生长（`backtrack.rs:201-205`），因此对任何"区域非全 2×2"的方块题**结构上不可能找到解**——6 道 UNSOLVED 的官方解区域大小各异（平均 3.6~13），全部命中。**`solitary` 同病**：`backtrack.rs:76-79`、`pieces.rs:137` 把独居误当"面积=1"，大概率也是 §4 中 13 道 compass+solitary UNSOLVED 的真凶（而非"搜索空间大"）。
2. **大棋盘 + 强规则组合下 aog 剪枝不足**：Rust 子进程在预算内解不出（首错均为 `rust timed out`）；且 exact_cover 启用门槛 est≤5000（`src/solver/exact_cover/solver.py:29-34`）挡掉了 0446（est=11025）、1109（est=6084）的纯 DLX 路径。1004 的 150s 硬超时另与 §5.3 回溯 rose-parallel 内存泄漏/线程不退出有关。

**不是建模/转换问题**——所有官方解都通过校验，只是求解器跑不出来。

### 已实施优化（2026-08-05 二次迭代，3 道已解出）

背景调研确认了两条路径：A) block→形状池转换（语义等价成立，`shape_pool` 允许复用/省略形状）；B) 修 Rust 的 block/solitary 建模错误。本轮两者都落地了：

1. **block → 形状池转换**：`src/solver/rust_solver.py` 在把 block 题发给 Rust 时，合成"所有矩形"（1×1…H×W）形状池注入载荷，让 `pieces`（DLX）接手。语义安全：最终答案仍由 router 用原 `block` 规则校验。
2. **修 Rust block/solitary 建模**：`backtrack.rs` / `pieces.rs` 移除 `block→4..4`、`solitary→1..1` 面积强制；`constraints.rs` 的 `check_block`（2×2 方块）→ `is_rectangle`（任意矩形）、`check_nonblock` → 非矩形、`check_same`/`check_different` → 按**形状**（原误按面积）、`check_solitary` → 每区域恰好 1 个线索格、`check_differentiation` → 邻区面积不同（与 aog 语义对齐）。backtrack 叶子检查补齐 block/different/solitary/differentiation。
3. **DLX 迭代到合法划分**：`dlx.rs` `search_with_check` 支持 on_solution 返回 true 中止；`pieces.rs` 用它 + `constraints::check_all` 逐个校验完整划分，跳过不满足 co-rule 的平凡划分（如全 1×1），直到找到合法矩形划分或超时。
4. **预算语义改为「单元预算」**：`timeout` 是每个求解器部分的预算，不是总分摊。`src/solver/base.py` 不再 `remaining/pending` 分摊，每个 Python 求解器各拿完整 `timeout`；Rust 侧 aog/pieces/backtrack 各拿完整 `timeout`（`mod.rs`），subprocess 给足 3× 墙钟（`rust_solver.py`）。
5. **形状池按盘面预筛**：`rust_solver.py::_fitting_rectangles` 只保留"至少有一个合法放置（不越障碍格、不跨预画边界）"的矩形，缩小注入形状池。对不规则盘面收益显著：0446 池 196→64（-67%）、1004 169→49（-71%）、0829 -60%、0826 -55%、0908v2 -61%；满盘面（1109）不变。求解速度小幅提升（0908v2 11.4→10.5s、0826 23.6→22.5s、0829 27.7→24.8s），池计算开销 ≤17ms。

**结果**（单元预算 20s，pytest ~387 全过，无回归）：

| 题 | 之前 | 现在 |
|---|---|---|
| 0908v2（differentiation+block+inequality，8×8） | UNSOLVED | ✅ 11.4s（rust/pieces DLX） |
| 0826（block+rose_window，10×10） | UNSOLVED | ✅ 23.6s（rust/pieces DLX） |
| 0829（block+area+solitary，11×11） | UNSOLVED | ✅ 27.7s（rust/pieces DLX） |
| 0446（different+block，14×14） | UNSOLVED | ❌ 209s（DLX 搜索 distinct 矩形划分超预算） |
| 1109（block+area+compass+solitary，12×12） | UNSOLVED | ❌ 120s（compass 强约束） |
| 1004（block+watchtower+rose_window，13×13） | UNSOLVED | ❌ 300s 硬超时（rose 组合 + §5.4 泄漏） |

其余 60 道（含 reference、0957、0835/0836）抽查全部照常解出。

**剩余 3 道的方向**：0446（distinct 矩形划分，DLX 可加形状去重剪枝）、1109（compass，需 compass 专项剪枝）、1004（rose+watchtower + 回溯 rose-parallel 内存泄漏，先修 §5.4）。

## 5. 后续计划

1. **甄别 6 道 watchtower DIFF**：在游戏侧实测；或深挖望塔规则在障碍格/边框附近的语义。
2. **提升求解能力**：compass+rose、ring 组合的剪枝；卡死题（回溯候选枚举超时）专项。
3. **✅ 修 Rust block/solitary 建模 + block→形状池转换（已实施）**：见 §4.1。方块题 63/66 可解。剩余：0446（DLX distinct 形状剪枝）、1109（compass 专项剪枝）、1004（见 4）。
4. **修回溯内存泄漏**：`backtrack._solve_rose_parallel` 的守护线程超时后不退出、累积内存，导致 `verify_puzzles.py` 全量跑 OOM（~990/1258）与 1004 等 rose 题 300s 不收敛。这是全量回归的阻塞项。
5. **回归基准**：**预算语义已改为「单元预算」（每部分各拿完整 timeout）**，影响全语料求解行为，需重跑全量扫描刷新 §2 数字（`scan_official_results.jsonl` 旧数字基于分摊预算）。每次优化后 `scripts/verify_puzzles.py` + 全量扫描。
6. **README / 文档同步**：见「软门禁」。

## 6. 软门禁（Soft Gate）

对以下任一模块的**每次优化**（修复、性能、规则语义、转换），提交前必须：

1. **更新本文档**：刷新 §2 进度数字；若 DIFF / UNSOLVED 集合有变化，更新 §3 / §4 并说明原因。
2. **更新对应文档**：`faq.md`（转换/校验经验）、`rules-guide.md`（规则语义）、`architecture.md`（求解架构）等，凡涉及处同步。
3. **README**：若影响外部可观察行为（命令、规则数、已知限制），同步 README。
4. **跑测试 + 验证**：`pytest`、`cargo test`、相关 `verify_puzzles.py` 片段，把结果记入本文档。

不满足即视为未完成，不应合入。

---
*最近更新：2026-08-05（方块题优化：修 Rust block/solitary 建模 + block→形状池转换 + 单元预算语义，方块题 60→63 可解；此前：gemini/delta 边约束、玫瑰窗检测、环纹边框 T 型、1SPR shape 修复）*
