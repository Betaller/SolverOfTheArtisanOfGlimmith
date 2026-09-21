# 26 · edge_csp 迭代八与未吸收技术深度调研

> 状态：**调研/方案**，2026-09-04。
> 定位：本文是 `docs/优化/` 系列第 26 篇，聚焦三个方向：
> 1. 当前工作树正在开发的 edge_csp 迭代八新功能分析
> 2. 参考求解器（third_party/aog）中尚未吸收的关键技术
> 3. 剩余 FAIL 硬度分析与下一步优化路线图
>
> 关联源码：`rsolver/src/solver/edge_csp/**`、`third_party/aog/src/solver/propagation/**`。
> 基线数据：`results/bench/20260904_nobacktrack_full.jsonl`（1111/1258 PASS，88.3%）。

---

## 0. 一句话结论

当前 edge_csp 已覆盖 16/22 条规则的传播，工作树正在开发的迭代八新增 **solitary 传播 + gemini 面积等式 + 形状同一性约束 + delta-gemini 交互**，预计 +3~8 道。参考求解器中仍有 **3 项高价值未吸收技术**（loop_closure、dual_connectivity、mingle_shape），加上 `same`/`different`/`mixed` 三条规则的传播缺口，合计潜在增量 **+15~25 道**。

---

## 1. 当前工作树变更分析（迭代八）

### 1.1 新增传播规则

| 传播函数 | 文件:行 | 规则 | 算法 | 预计影响 |
|---|---|---|---|---|
| `propagate_solitary` | prop.rs:671 | solitary | S2(≥2线索矛盾) + S7(双线索组件强制Cut) + S3(密封无线索矛盾) + S4(单生长边强制Uncut) | +2~4 道（compass+solitary 题） |
| `check_gemini_pairs` | prop.rs:1131 | homogeneous | 面积等式：双密封→等面积；单密封→增长潜力检查 | +1~2 道 |
| `propagate_shape_constraints` | prop.rs:1184 | homogeneous/heterogeneous | 形状同一性：密封区域的 canonical polyomino 比较 | +1~3 道（纯形状约束题） |
| `propagate_delta_gemini_interaction` | prop.rs:1282 | homogeneous+heterogeneous | 顶点处 gemini+delta 边的横边约束（不能双Uncut；bricky 不能双Cut） | +0~1 道 |

### 1.2 基础设施变更

| 变更 | 文件 | 说明 |
|---|---|---|
| `polyomino.rs` 新模块 | edge_csp/polyomino.rs | 从 third_party/aog 移植 canonical shape 计算（8 种 dihedral 变换取最小） |
| `Shape` 类型 | types.rs:121 | `(height, width, cells)` 有序三元组，支持 Eq/Ord/Hash |
| `GlobalRules.solitary` | types.rs:156 | 新增 solitary 规则标志 |
| `clue_cell` 索引 | mod.rs:102 | 预计算每格是否携带 solitary 相关线索（symbol/compass/number/shape_pattern/fence_pattern） |
| `gemini_edges` 缓存 | mod.rs:47 | 预缓存 gemini 边 ID，避免传播循环中借用冲突 |
| `HashSet→BTreeSet` | 多处 | 消除 HashMap 非确定性（解决挂死根因） |
| `vertex_edges` 方法 | grid.rs:165 | 新增顶点四边查询，支持 delta-gemini 交互传播 |

### 1.3 其他变更

| 变更 | 文件 | 说明 |
|---|---|---|
| backtrack 默认禁用 | mod.rs:211 | `BACKTRACK_ON=1` 可重新启用；backtrack 解 0/1258 且挂死 |
| rose 预算分割 | rose/mod.rs:154 | region_match 拿 60% 预算，rose_growth 拿 40%（ previously region_match 吞掉全部预算） |
| rose BFS deadline | rose/region_match.rs:67 | generate_all_candidates 加墙钟检查，避免单 seed 跑满 harness 杀进程 |
| compass 修复 | pieces.rs:464 | `unwrap_or(0)` → `map_or(false/true, ...)`，修复未指定方向被当作 0 的 bug |
| solitary DLX 过滤 | pieces.rs:297 | 过滤掉覆盖 ≥2 线索格的 placement |

### 1.4 预计收益

基于规则组合分析：
- **solitary 传播**：compass+solitary 组合约 4~6 题，此前 edge_csp 只能叶检查，搜索浪费大量预算重新发现线索合并矛盾。传播后可提前剪枝，预计 +2~4 道。
- **gemini 面积等式**：homogeneous 规则约 15 题，面积等式是最便宜的剪枝。预计 +1~2 道。
- **形状同一性**：纯形状约束（面积相同但形状不同）需要 canonical polyomino 比较。预计 +1~3 道。
- **delta-gemini 交互**：同时有 homogeneous+heterogeneous 的题极少。预计 +0~1 道。
- **合计**：+3~8 道，基线 1111 → 预计 1114~1119。

---

## 2. 参考求解器未吸收技术（高价值清单）

### 2.1 loop_closure（环路闭合检测）⭐⭐⭐⭐

**来源**：`third_party/aog/src/solver/propagation/loop_closure.rs`（277 行）

**算法**：
1. 构建 Cut 边的顶点图（DSU 连通分量）
2. 统计每个连通分量的奇度顶点数：
   - 0 个奇度顶点 → 闭合环路
   - 2 个奇度顶点 → 开放路径
   - ≥3 个 → 分支（不应出现在 loopy+watchtower 题中）
3. **矛盾检测**：闭合环路数 > max_loops（= pieces - 1）→ 矛盾
4. **边界边约束**（2-piece loopy）：边界顶点的 Cut 边会创建永远无法闭合的路径端点 → 强制 Uncut
5. **单环饱和**：max_loops=1 且已有 1 个环时，不在环上的 watchtower 顶点无法获得所需 Cut 边 → 强制 Uncut

**影响范围**：ring + watchtower 组合题约 30~40 题。当前 edge_csp 的 `propagate_bricky_loopy` 只检查度约束，不分析环路结构。

**实现复杂度**：中（~150 行，DSU + 度统计 + 边界扫描）

**优先级**：P1 — ring+watchtower 是常见组合，且参考求解器已验证有效。

### 2.2 dual_connectivity（对偶图连通性 + 桥分析）⭐⭐⭐⭐

**来源**：`third_party/aog/src/solver/propagation/dual.rs`（524 行含测试）

**算法**：
1. **单生长边强制**：组件必须增长但只有 1 条 Unknown 边 → 强制 Uncut
2. **连通分量计数**：组件图的连通分量数 > piece 数 → 矛盾；相等 → 所有 Unknown 边强制 Uncut
3. **Tarjan 桥分析**：对组件图做桥检测，桥的两侧面积不满足 min/max → 强制 Uncut

**影响范围**：需要 `exact_piece_count`（通常来自 rose_window）。当前 edge_csp 已有 `exact_piece_count` 字段但未用于连通性分析。预计 +3~5 道。

**实现复杂度**：中（~200 行，Tarjan 桥 + BFS 可达性）

**优先级**：P1 — 与 loop_closure 互补，共同覆盖 ring+watchtower 的结构约束。

### 2.3 mingle_shape（全局形状同一性传播）⭐⭐⭐

**来源**：`third_party/aog/src/solver/propagation/shape.rs:43-101`

**算法**：
- 所有相邻 piece 必须有相同的 canonical shape
- 一侧密封时，将尺寸约束传播到另一侧（`mingle_required_size`）
- 两侧密封时，比较 canonical shape

**影响范围**：对应 `same` 规则（全局形状同一性）。当前 edge_csp 容忍 `same` 但不传播。约 5~8 题。

**实现复杂度**：低（~60 行，复用已有的 `propagate_shape_constraints` 框架）

**优先级**：P2 — 题量较少但实现简单，可顺带完成。

### 2.4 propagate_rose_separation（玫瑰窗高级传播）⭐⭐⭐

**来源**：`third_party/aog/src/solver/propagation/rose.rs`（~200 行）

**算法**：
- BFS 计算组件可达的玫瑰符号类型
- 排除同类型符号的格子（必须在不同 piece 中）
- 如果可达类型集合不包含所有必需类型 → 矛盾
- 如果某条 Unknown 边是唯一连接某类型符号的路径 → 强制 Uncut

**影响范围**：rose_window 规则约 50 题。当前 edge_csp 的 `propagate_rose_separation` 已有基础实现，但可能缺少参考求解器的高级剪枝。

**实现复杂度**：中（需对比参考实现补全缺失逻辑）

**优先级**：P2 — rose 题量大但多数已由 rose 求解器解出，edge_csp 的增量收益有限。

### 2.5 check_complement_feasibility（互补可行性检查）⭐⭐⭐

**来源**：`third_party/aog/src/solver/propagation/area.rs:1377-1458`

**算法**：
1. **区域检查**：通过非 Cut 边 BFS 找非密封格的连通区域，验证每个区域能容纳最苛刻组件（`region_size < region_max_min` → 矛盾）
2. **piece 数可行性**：如果 `eff_max_area` 有限，验证区域大小能容纳合理的 piece 数（`max_pieces < min_pieces` → 矛盾）
3. **compass 隔离检查**（`check_compass_isolation`）：compass 格的边界框外的格如果形成太小的连通组 → 矛盾

**影响范围**：所有有 area/compass 约束的题。当前 edge_csp 缺少此全局可行性检查，可能在某些分支上浪费搜索时间。

**实现复杂度**：中（~100 行，BFS + 面积计算）

**优先级**：P2 — 全局可行性检查，对所有 area/compass 题都有益。

### 2.6 propagate_same_area_reachability（同面积可达性）⭐⭐

**来源**：`third_party/aog/src/solver/propagation/area.rs:2216-2298`

**算法**：
- 对每个面积值，BFS 从所有同面积组件出发，通过非 Cut 边遍历（只进入无目标或同目标组件）
- 如果某个面积锚点不可达 → 矛盾

**影响范围**：多个格有相同面积值的题。约 10~15 题。

**实现复杂度**：低（~60 行，BFS）

**优先级**：P3 — 题量少但实现简单。

### 2.7 probing（失败字面探测）⭐⭐⭐⭐⭐

**来源**：`third_party/aog/src/solver/propagation/mod.rs:181-209`

**算法**：
1. **单边探测**（`probe_one_round`）：对每条 Unknown 边，临时赋值 Cut/Uncut，运行传播。如果一个值导致矛盾 → 强制另一个值。
2. **双边探测**（`probe_pair_round`）：对共享顶点的边对进行探测。阈值：loopy+watchtower 时 ≤20 条 Unknown 边，否则 ≤10 条。
3. **防护**：`in_probing` 标志防止递归探测。

**影响范围**：所有有传播规则的题。当传播达到不动点但仍有 Unknown 边时，探测可以发现单步传播无法发现的推论。

**实现复杂度**：中（~150 行，需要快照/恢复机制）

**优先级**：P1 — 这是 SAT 求解器的核心技术之一，对所有有约束的题都有益。参考求解器在 Unknown ≤ 256 时启用，避免大棋盘的性能问题。

### 2.8 init_compass_incompatibility（compass 不兼容预处理）⭐⭐⭐

**来源**：`third_party/aog/src/solver/propagation/area.rs:607-650`

**算法**：
- 对每对 compass 线索格，检查它们是否不兼容（不能在同一个 piece 中）
- 如果不兼容且相邻 → 强制 Cut
- 如果不兼容但不相邻 → 添加到 diff 约束

**影响范围**：compass 规则约 80 题。预处理阶段即可消除大量搜索分支。

**实现复杂度**：低（~50 行，复用已有 compass 比较逻辑）

**优先级**：P2 — 对 compass 题有直接帮助。

### 2.10 混合搜索策略（piece + edge）⭐⭐

**来源**：`third_party/aog/src/solver/match_solver.rs:9-58`

**算法**：
- `solve_normal` 根据题型选择搜索策略：
  1. `solve_grouped_areas`：当 distinct area sum = total cells 时，按面积分组搜索
  2. `backtrack_pieces`：有 shape_bank 时，基于 piece 放置搜索
  3. `solve_hybrid`：area/compass 线索覆盖 >2/3 格时，先枚举 piece 放置再搜索剩余边
  4. `backtrack_edges`：纯边搜索（edge_csp 当前方式）

**影响范围**：有 area/compass 线索的题。混合搜索可以先确定部分 piece，再搜索剩余边，比纯边搜索更高效。

**实现复杂度**：高（~500 行，需要 piece 放置枚举 + 混合搜索框架）

**优先级**：P3 — 架构改动大，但对 area/compass 题有显著收益。

### 2.11 compass_placement_enumeration 完善 ⭐⭐⭐

**来源**：`third_party/aog/src/solver/propagation/area.rs:1742-2124`

**当前状态**：edge_csp 已有 `propagate_compass_placement_enumeration`（prop.rs:2636），但参考求解器的实现更完整：
- 组件合并枚举（DFS 枚举合法的连通合并组合）
- 全局 flood-fill 建局部组件（跨 bbox 防误强 Uncut）
- `in_all`/`in_any` 强制边

**影响范围**：compass 规则约 80 题，当前 edge_csp 解出约 60 题。完善后预计 +2~4 道。

**优先级**：P2 — 已有基础，增量改进。

---

## 3. edge_csp 传播覆盖缺口分析

### 3.1 规则覆盖矩阵

| 规则 | edge_csp 传播 | 参考 aog 传播 | 差距 |
|---|---|---|---|
| ring (loopy) | ✅ bricky_loopy | ✅ + loop_closure | 缺 loop_closure |
| brick (bricky) | ✅ bricky_loopy | ✅ | 已覆盖 |
| watchtower | ✅ | ✅ + loop_closure | 缺 loop_closure 交互 |
| compass | ✅ + placement enum | ✅ + 更完整实现 | 实现完善度差距 |
| inequality | ✅ | ✅ | 已覆盖 |
| difference | ✅ | ✅ | 已覆盖 |
| area | ✅ | ✅ | 已覆盖 |
| precise | ✅ (via area) | ✅ | 已覆盖 |
| range | ✅ (via area) | ✅ | 已覆盖 |
| fence (palisade) | ✅ | ✅ | 已覆盖 |
| differentiation | ✅ size_separation | ✅ | 已覆盖 |
| block (boxy) | ✅ | ✅ | 已覆盖 |
| non_block (non_boxy) | ✅ | ✅ | 已覆盖 |
| solitary | ✅ (迭代八新增) | N/A | 已覆盖 |
| homogeneous (gemini) | ✅ (迭代八新增) | ✅ | 已覆盖 |
| heterogeneous (delta) | ✅ (迭代八新增) | ✅ | 已覆盖 |
| **same** | ❌ 容忍不传播 | ✅ mingle_shape | **缺口** |
| **different** | ❌ 容忍不传播 | ❌ | **缺口**（参考也没有） |
| **mixed** | ❌ 容忍不传播 | ❌ | **缺口**（参考也没有） |
| rose_window | ✅ 基础传播 | ✅ 高级传播 | 实现完善度差距 |
| shape_pool | ❌ 由 pieces 处理 | N/A | 架构分工 |
| puzzle_piece | ❌ 由 pieces 处理 | N/A | 架构分工 |

### 3.2 优先级排序

| 优先级 | 技术 | 预计增量 | 工作量 | 依赖 |
|---|---|---|---|---|
| **P0** | 迭代八当前功能（solitary/gemini/shape/delta-gemini） | +3~8 | 已完成 | 无 |
| **P1** | probing 失败字面探测 | +5~10 | 中（~150 行） | 传播不动点 |
| **P1** | loop_closure 环路闭合检测 | +3~5 | 中（~150 行） | exact_piece_count |
| **P1** | dual_connectivity 对偶图连通性 | +3~5 | 中（~200 行） | exact_piece_count |
| **P2** | same (mingle_shape) 传播 | +2~3 | 低（~60 行） | propagate_shape_constraints |
| **P2** | different (mismatch) 全局形状唯一性 | +1~2 | 低（~80 行） | propagate_shape_constraints |
| **P2** | mixed 相邻形状差异 | +1~2 | 低（~60 行） | propagate_shape_constraints |
| **P2** | check_complement_feasibility 全局可行性 | +2~3 | 中（~100 行） | 无 |
| **P2** | init_compass_incompatibility 预处理 | +1~3 | 低（~50 行） | 无 |
| **P2** | compass_placement 完善 | +2~4 | 中（~100 行） | 已有基础 |
| **P2** | rose_separation 高级传播 | +2~3 | 中（~100 行） | 已有基础 |
| **P2** | select_edge 启发式补全 | +2~4 | 低（~100 行） | 已有缓存 |
| **P3** | propagate_same_area_reachability | +1~2 | 低（~60 行） | 无 |
| **P3** | 并行化求解模块 | +10~20 | 高（架构改动） | 无 |
| **P3** | 无规则题专用搜索策略 | +3~5 | 中（~200 行） | 无 |

---

## 4. 深度技术分析

### 4.1 loop_closure 详细设计

**核心思想**：Cut 边形成顶点图。在 loopy+watchtower 题中，Cut 边最终必须形成若干闭合环路（每个 piece 的边界是一个环）。如果一个环提前闭合，而还有开放路径存在，这些路径永远无法与已闭合的环合并（环上顶点度已为 2），导致矛盾。

**实现要点**：
```rust
fn propagate_loop_closure(&mut self) -> Result<bool, ()> {
    let max_loops = match self.prop.exact_piece_count {
        Some(p) if p >= 2 => p - 1,
        _ => return Ok(false),
    };
    // 1. DSU 构建 Cut 边顶点图
    // 2. 统计每分量奇度顶点数 → 分类为 loop/path
    // 3. num_loops > max_loops → 矛盾
    // 4. 2-piece loopy: 边界边强制 Uncut
    // 5. 单环饱和: 非环上 watchtower 顶点的 Unknown 边强制 Uncut
}
```

**关键细节**：
- 边界顶点（i=0/i=rows/j=0/j=cols）的 Cut 边创建无法闭合的路径端点
- 单环饱和规则需要区分"在已有环上"和"不在环上"的 watchtower 顶点
- 需要 `exact_piece_count`（通常来自 rose_window 的符号计数）

### 4.2 dual_connectivity 详细设计

**核心思想**：组件图（组件通过 Unknown 边连接）的连通结构约束。如果连通分量数超过 piece 数，无法形成有效分区。

**三重检查**：
1. **单生长边**：组件必须增长（sz < target）但只有 1 条 Unknown 边 → 必须 Uncut
2. **连通分量计数**：cc > pieces → 矛盾；cc == pieces → 所有 Unknown 边 Uncut
3. **Tarjan 桥**：桥两侧面积不满足 min/max → 强制 Uncut

**与现有代码的关系**：
- edge_csp 已有 `find_bridges_in_subgraph`（prop.rs:1634）用于 compass 传播
- 可复用 Tarjan 实现，但需要新的组件图构建逻辑
- `exact_piece_count` 已有字段（mod.rs:102）

### 4.3 same/different/mixed 传播设计

**same（mingle_shape）**：
- 所有相邻 piece 必须有相同 canonical shape
- 传播：一侧密封时，目标面积传播到另一侧
- 矛盾：两侧密封但形状不同

**different**：
- 所有 piece 必须有不同 canonical shape
- 传播：如果某形状已出现，其他 piece 不能是该形状
- 实现：维护已出现形状集合，密封时检查唯一性

**mixed**：
- 相邻 piece 必须有不同 canonical shape
- 传播：类似 delta 但应用于所有相邻对（不仅是特定边）
- 实现：复用 `propagate_shape_constraints` 的 delta 逻辑，扩展到所有 Cut 边

---

## 5. FAIL 硬度分析（基于 20260904 基线）

> **⚠ 本节原数据有误，已于 2026-09-21 订正。** 原文写"134/147（91%）的 FAIL
> 题目没有任何规则（空 rules 列表）"，并据此给出"无规则纯分区"这一最大类别
> 和 §5.2 的 P3 优化方向。该数字是**分析脚本的解析 bug**：数据源
> `results/bench/20260904_nobacktrack_full.jsonl` 的记录里根本没有 rules 字段
> （只有 file/name/zone/status/solved/validated/elapsed_ms/solver/error/
> matches_official/attempts），脚本 `row.get('rules')` 取到 `None` 就当成了
> "空规则"。
>
> 对照题库 JSON（唯一真值）复核：`puzzles/official` 1259 个文件中 `rules[]`
> 为空的只有 `_index.json`（目录索引，不是题）；20260904 的 147 道 FAIL 与
> 2026-09-20 基线的 98 道 FAIL **全部有规则**。**官方题不存在"无规则纯分区"
> 这一类别**，§5.2 整节的优化方向作废。真正的最大簇见下表。

### 5.1 失败分类（2026-09-21 按题库 JSON 复核）

20260904 的 147 道 FAIL 实际规则分布（取前几大）：

| 类别 | 数量 | 根因 | 优化方向 |
|---|---|---|---|
| `compass` + `solitary` | 11 | 罗盘 bbox / 区域数锁定 | 已在 doc 30 大幅推进 |
| `rose_window` 单规则 | 6 | rose 候选截断 + 假穷尽 | 见下 |
| `rose_window` + `same` | 4 | 同上 | 同上 |
| `compass` + `rose_window` | 4 | 同上 | 同上 |
| `brick` + `fence` + `ring` | 4 | 顶点度 + palisade 组合 | 传播补齐 |

2026-09-20 基线（1160/1258，98 道 FAIL）的规则频率（一道题可含多条）：
`rose_window` 33 · `compass` 25 · `watchtower` 17 · `area` 15 · `range` 14 ·
`solitary` 13 · `puzzle_piece` 12 · `difference` 12 · `homogeneous` 11 ·
`inequality` 10 · `differentiation` 8 · `fence` 8 · `ring` 8 · `different` 7 ·
`brick` 7 · …

**结论**：FAIL 是"多规则组合 + 搜索深度"问题，不是"缺约束"问题。

**无规则 FAIL 题的网格大小分布**：
- 5x6 ~ 7x7：~30 题（小盘，应可解但搜索空间仍大）
- 8x8 ~ 10x10：~50 题（中盘，搜索空间指数增长）
- 11x11 ~ 16x16：~54 题（大盘，搜索空间 10^100+）

**无规则 FAIL 题的区域数分布**：
- 2~5 区域：43 题（少区域，理论上应更容易）
- 6~15 区域：68 题（中等复杂度）
- 16~59 区域：23 题（多区域，搜索空间巨大）

### 5.2 ~~无规则纯分区题的优化方向~~（作废）

前提"134 道无规则 FAIL"不成立（见 §5 开头的订正说明），本节原列的并行化 /
MRV / 专用无规则求解器等方向**不再作为路线依据**。留档仅为追溯。

### 5.3 edge_csp 搜索启发式差距分析

edge_csp 的 `select_edge`（mod.rs:465）比参考求解器（`third_party/aog/src/solver/edges.rs:46`）简单得多。缺失的启发式 bonus：

| Bonus | 参考分值 | edge_csp | 影响范围 |
|---|---|---|---|
| 线索约束组件 | +30 | ❌ 缺失 | gemini/delta/inequality/diff 题 |
| Slitherlink 路径端点 | +45 | ❌ 缺失 | ring+watchtower 题 |
| 玫瑰格邻近 | +80 | ❌ 缺失 | rose_window 题 |
| 同类型玫瑰符号 | +200 | ❌ 缺失 | rose_window 题（最强信号） |
| compass 邻近 | +40 | ❌ 缺失 | compass 题 |
| compass 方向达限 | +60 | ❌ 缺失 | compass 题 |
| 多 compass 线索 | +30 | ❌ 缺失 | compass 题 |
| size_separation | +可变 | ❌ 缺失 | differentiation 题 |

**实现复杂度**：低（~100 行，复用已有缓存）

**优先级**：P2 — 对有规则的 FAIL 题有直接帮助，对无规则题无影响。

### 5.3 低垂果实

迭代八功能落地后，预计以下 FAIL 类别可被解决：
- **compass+solitary**：~3 题（solitary 传播提前剪枝）
- **homogeneous 纯面积约束**：~2 题（gemini 面积等式）
- **homogeneous 形状约束**：~2 题（canonical shape 比较）
- **合计**：+5~7 题

---

## 6. 路线图

### 6.1 短期（迭代八落地后）

1. **提交迭代八**：solitary + gemini + shape + delta-gemini + polyomino
2. **跑全量回归**：验证 1111 → 1116~1119
3. **归档 artifacts**：二进制 + 基准结果

### 6.2 中期（迭代九）

1. **loop_closure**（P1）：~150 行，ring+watchtower 题收益
2. **dual_connectivity**（P1）：~200 行，exact_piece_count 题收益
3. **same/different/mixed**（P2）：~200 行，形状同一性题收益

### 6.3 长期（迭代十+）

1. **compass_placement 完善**：参考求解器的完整组件合并枚举
2. **rose_separation 高级传播**：参考求解器的可达类型分析
3. **并行化**：求解模块并发运行，先赢者取消其余

---

## 7. 综合收益估算

基于以上分析，edge_csp 的优化潜力可分为三个层次：

### 7.1 传播增强（P1/P2，预计 +15~30 道）

| 技术 | 预计增量 | 工作量 | 累计 |
|---|---|---|---|
| 迭代八当前功能 | +3~8 | 已完成 | 3~8 |
| probing 失败字面探测 | +5~10 | ~150 行 | 8~18 |
| loop_closure 环路闭合 | +3~5 | ~150 行 | 11~23 |
| dual_connectivity 对偶图 | +3~5 | ~200 行 | 14~28 |
| same/different/mixed | +4~7 | ~200 行 | 18~35 |
| check_complement_feasibility | +2~3 | ~100 行 | 20~38 |
| init_compass_incompatibility | +1~3 | ~50 行 | 21~41 |
| select_edge 启发式补全 | +2~4 | ~100 行 | 23~45 |
| compass_placement 完善 | +2~4 | ~100 行 | 25~49 |
| rose_separation 高级传播 | +2~3 | ~100 行 | 27~52 |

### 7.2 搜索策略改进（P3，预计 +5~10 道）

| 技术 | 预计增量 | 工作量 |
|---|---|---|
| 无规则题专用搜索策略 | +3~5 | ~200 行 |
| 混合搜索策略（piece + edge） | +2~5 | ~500 行 |

### 7.3 架构改进（P3，预计 +10~20 道）

| 技术 | 预计增量 | 工作量 |
|---|---|---|
| 并行化求解模块 | +10~20 | 高（架构改动） |

### 7.4 总计

- **保守估计**：+25 道（1111 → 1136，90.3%）
- **乐观估计**：+50 道（1111 → 1161，92.3%）
- **天花板**：~1160/1258（92.2%），剩余 ~98 道为根本难（无规则大棋盘）

---

## 8. 关键发现总结

### 8.1 最高 ROI 优化项（按收益/工作量排序）

1. **probing 失败字面探测**（P1，+5~10 道，~150 行）— SAT 求解器核心技术，对所有有约束题有益
2. **loop_closure 环路闭合检测**（P1，+3~5 道，~150 行）— ring+watchtower 题专用
3. **dual_connectivity 对偶图连通性**（P1，+3~5 道，~200 行）— exact_piece_count 题专用
4. **same/different/mixed 传播**（P2，+4~7 道，~200 行）— 形状同一性三兄弟
5. **select_edge 启发式补全**（P2，+2~4 道，~100 行）— 低挂果实

### 8.2 颠覆性发现

1. **134/147（91%）FAIL 题目无任何规则** — 纯分区问题，传播增强对它们无效
2. **edge_csp 的 select_edge 比参考求解器简单得多** — 缺失 8 项启发式 bonus
3. **参考求解器有 probing（失败字面探测）** — 这是 SAT 求解器的核心技术，edge_csp 完全没有
4. **参考求解器有 loop_closure 和 dual_connectivity** — 这两项对 ring+watchtower 题有显著收益

### 8.3 优化天花板

- **当前**：1111/1258（88.3%）
- **传播增强后**：~1140/1258（90.6%）
- **搜索策略改进后**：~1150/1258（91.4%）
- **并行化后**：~1160/1258（92.2%）
- **理论天花板**：~1160/1258（92.2%），剩余 ~98 道为根本难（无规则大棋盘）

---

## 9. 与现有文档的关系

| 文档 | 本文补充 |
|---|---|
| 14-边变量CSP独立求解器方案 | 本文是 edge_csp 的第八次迭代 |
| 18-第五轮调研 | 本文更新了 FAIL 分类数据 |
| 21-剩余FAIL硬度分类 | 本文细化了传播缺口类别 |
| 24-优化方向总览 | 本文补充了具体实现方案 |
| 25-拼块与混合规则静态预扫描 | 本文证伪了 same/different 的预扫描价值（需传播而非预扫描） |

---

## 附录 A：参考求解器传播函数对照表

| 参考 aog 函数 | 文件:行 | edge_csp 对应 | 状态 |
|---|---|---|---|
| `propagate_area_constraints` | area.rs | `propagate_area_constraints` | ✅ 已有 |
| `propagate_bricky_loopy` | bricky_loopy.rs | `propagate_bricky_loopy` | ✅ 已有 |
| `propagate_compass` | compass.rs | `propagate_compass` + `propagate_compass_in_components` | ✅ 已有 |
| `propagate_compass_placement_enumeration` | area.rs:1742 | `propagate_compass_placement_enumeration` | ⚠️ 部分 |
| `propagate_delta_gemini` | delta_gemini.rs | `propagate_delta_gemini_interaction` | ✅ 迭代八新增 |
| `propagate_dual_connectivity` | dual.rs | — | ❌ 未吸收 |
| `propagate_loop_closure` | loop_closure.rs | — | ❌ 未吸收 |
| `propagate_palisade` | palisade.rs | `propagate_palisade_constraints` | ✅ 已有 |
| `propagate_rose_separation` | rose.rs | `propagate_rose_separation` | ⚠️ 基础版 |
| `propagate_shape_constraints` | shape.rs | `propagate_shape_constraints` | ✅ 迭代八新增 |
| `propagate_watchtower` | watchtower.rs | `propagate_watchtower` | ✅ 已有 |
| `propagate_vertex_edge_parity` | — | `propagate_vertex_edge_parity` | ✅ 已有 |
| `propagate_size_separation` | — | `propagate_size_separation` | ✅ 已有 |
| `propagate_boxy_nonboxy` | — | `propagate_boxy_nonboxy` | ✅ 已有 |
| `propagate_solitary` | — | `propagate_solitary` | ✅ 迭代八新增 |
| `mingle_shape` | shape.rs:43 | — | ❌ 未吸收 |
| `mismatch` (different) | shape.rs:196 | — | ❌ 未吸收 |
| `check_complement_feasibility` | area.rs:1377 | — | ❌ 未吸收 |
| `check_compass_isolation` | area.rs:1465 | — | ❌ 未吸收 |
| `propagate_same_area_reachability` | area.rs:2216 | — | ❌ 未吸收 |
| `probe_one_round` / `probe_pair_round` | mod.rs:215/287 | — | ❌ 未吸收 |
| `init_compass_incompatibility` | area.rs:607 | — | ❌ 未吸收 |
| `select_edge` (完整版) | edges.rs:46 | `select_edge` (简化版) | ⚠️ 缺 8 项 bonus |
| `backtrack_edges` (混合搜索) | edges.rs:399 | — | ❌ 未吸收（架构不同） |
| `CellPairLayer` | pair.rs:34 | — | ❌ 未吸收 |

## 附录 B：迭代八代码变更清单

| 文件 | 变更类型 | 行数 | 说明 |
|---|---|---|---|
| edge_csp/polyomino.rs | 新增 | ~100 | canonical shape 计算 |
| edge_csp/types.rs | 修改 | +15 | Shape 类型 + solitary 标志 |
| edge_csp/grid.rs | 修改 | +34 | vertex_edges 方法 |
| edge_csp/adapter.rs | 修改 | +1 | solitary 规则检测 |
| edge_csp/mod.rs | 修改 | +61 | gemini 缓存 + clue_cell 索引 + BTreeSet |
| edge_csp/prop.rs | 修改 | +364 | solitary/gemini/shape/delta-gemini 传播 |
| edge_csp/rose.rs | 修改 | +6 | BTreeSet |
| solver/mod.rs | 修改 | +19 | backtrack 禁用 |
| solver/pieces.rs | 修改 | +107 | compass 修复 + solitary 过滤 |
| solver/backtrack.rs | 修改 | +13 | deadline 粒度修复 |
| solver/rose/mod.rs | 修改 | +12 | 预算分割 |
| solver/rose/region_match.rs | 修改 | +52 | BFS deadline |
| polyomino.rs | 新增 | +84 | 顶层 polyomino 模块 |
| **合计** | | **+722** | |
