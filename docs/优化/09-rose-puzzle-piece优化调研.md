# 09 · rose 求解器 puzzle_piece 约束集成调研

> **背景**：当前 rose 求解器的 `region_match` 遇到 `puzzle_piece` 或 `shape_pool`
> 规则直接放弃（`region_match.rs:288-290`），导致 puzzle_piece + rose_window 组合题
> 在 aog 3s 预算用尽后无有效求解路径。共 **10 道**此类题，当前基本全超时。
>
> 更新日期：2026-08-07

---

## 0. 一句话结论

**puzzle_piece 是玫瑰窗的最强约束**——它直接锁定了某个区域的精确形状。
将 puzzle_piece 形状的 8 种朝向作为候选，替代 BFS 枚举，可将搜索空间从
「每 seed 最多 20000 候选」降到「最多 8 候选」。改造难度中等，预计可解出
全部 10 道组合题。

---

## 1. 受影响题目

| 题号 | 网格 | 规则 | 当前状态 |
|------|------|------|---------|
| 0732 | 7×7 | puzzle_piece + rose_window (3 类符号) | 超时 |
| 1096 | 7×7 | puzzle_piece + rose_window | 超时 |
| 0225 | 9×9 | puzzle_piece + rose_window | 超时 |
| 0226 | 7×9 | puzzle_piece + rose_window | 超时 |
| 0224 | 9×13 | puzzle_piece + rose_window | 超时 |
| 1343 | 6×6 | puzzle_piece + range + rose_window | 超时 |
| 0344r | 7×6 | puzzle_piece + area + rose_window | 超时 |
| 1098 | 9×8 | puzzle_piece + **homogeneous** + rose_window | 超时 |
| 1099 | 9×10 | puzzle_piece + **homogeneous** + rose_window | 超时 |
| 1100 | 13×13 | puzzle_piece + **homogeneous** + rose_window | 超时 |

---

## 2. 当前求解路径分析（以 0732 为例）

### 2.1 0732 结构

```
7×7 网格, 0 blocked, 49 fillable
符号: P1×2, P2×2, P3×2  → M = 2 区域
puzzle_piece: 1 个, 位于 (3,3), 形状大小 9 (5×3 包围盒)
```

两个区域:
- 区域 A（含 puzzle_piece): 9 格，形状必须匹配 (3,3) 处的图案
- 区域 B（另一区域): 40 格，必须连通，含 1 P1 + 1 P2 + 1 P3

> **M=2 快速路径**：当 M=2 且只有一个 puzzle_piece 时，两个区域互为补集。
> puzzle_piece 的 8 个朝向中只需找到一个满足符号分布约束（恰好每种符号一个）的放置，
> 剩余 40 格自动成为第二个区域。不需要跑 BFS 候选生成 / MRV 匹配 / 面积组合枚举，
> O(8) 检查即可出解。

### 2.2 当前求解路径

```
1. AoG: rose-capable → 3s 预算 → 自由形状枚举 → 3s 耗尽, fail
2. rose::region_match: 检测到 puzzle_piece → 直接 return None
3. rose::rose_growth (兜底): 启发式 → 大概率失败
4. pieces/backtrack: 缺少玫瑰窗剪枝 → 超时
```

### 2.3 根因

`region_match.rs:285-291`:
```rust
if puzzle.rules.iter().any(|r| r.ctype == "shape_pool" || r.ctype == "puzzle_piece") {
    return None;  // ← 这里直接放弃
}
```

放弃原因：当前 region_match 通过 BFS 从每个 seed 符号格子出发枚举所有连通区域候选，
不感知 puzzle_piece 的形状约束。如果 puzzle_piece 存在，BFS 生成的候选可能不满足
形状约束，导致后续匹配失败。

---

## 3. 优化方案

### 3.1 核心思路

**用 puzzle_piece 形状的 8 种 D4 朝向替代 BFS 候选枚举。**

对于每个 puzzle_piece 格子：
- 取出其 `shape_pattern`（相对坐标列表）
- 生成 8 种朝向（4 旋转 × 2 镜像）
- 以该格子为锚点，将每种朝向放置到网格上
- 筛选：放置必须全部在网格内、不覆盖 blocked 格、符号约束满足
- 结果：最多 8 个候选区域（而非 BFS 的数千个）

### 3.2 候选筛选条件

对 puzzle_piece 区域的 8 个朝向，每个朝向必须满足：

1. **边界内**: 所有格子在 `[0, H) × [0, W)` 内
2. **无 blocked**: 所有格子非 blocked
3. **符号约束**: 区域恰好包含所需符号
   - 纯 rose_window: 恰好每种符号 1 个
   - rose_window + area: 区域面积 = area 值
   - rose_window + range: 区域面积 ∈ [min, max]
   - rose_window + homogeneous: 区域形状与其他 Gemini 边相连区域相同
4. **不重叠**: 多个 puzzle_piece 区域不能重叠（若存在多个 puzzle_piece）

### 3.3 混合求解流程

```
输入: puzzle (含 puzzle_piece + rose_window)

Phase 1: 生成 puzzle_piece 区域候选
  for each puzzle_piece cell:
    for each of 8 orientations:
      validate(candidate) → accept/reject
  结果: 每 puzzle_piece cell 最多 8 个候选

Phase 2: 候选组合（若多个 puzzle_piece）
  枚举所有 puzzle_piece 候选的笛卡尔积
  排除重叠的组合 → 得到「已确定区域」集合

Phase 3: 处理剩余区域（无 puzzle_piece 约束的 rose 区域）
  剩余格子 = 全部格子 - 已确定区域占用的格子
  对剩余格子:
    - 若还有未分配的符号 → 剩余区域仍需满足 rose_window
    - 使用现有 region_match BFS 枚举（搜索空间已大幅缩小）

Phase 4: 验证
  用现有 validate 检查完整解
```

### 3.4 具体实现

在 `rose/region_match.rs` 中，将 `return None` 替换为：

```rust
if has_puzzle_piece {
    // Generate puzzle-piece-constrained candidates
    let pp_candidates = generate_puzzle_piece_candidates(puzzle);
    if pp_candidates.is_empty() {
        return None; // puzzle piece shape doesn't fit anywhere
    }
    
    // For each valid combination, solve remaining rose regions
    for combo in enumerate_combinations(&pp_candidates) {
        let remaining = all_positions.clone().subtract(&combo.cells());
        let remaining_symbols = filter_unplaced_symbols(symbol_types, &combo);
        
        // Solve remaining with standard region_match (BFS from seeds)
        if let Some(regions) = solve_remaining_rose(
            puzzle, &pre, &remaining_symbols, m - combo.len(),
            &remaining, start, remaining_timeout,
        ) {
            return Some(combo.merge(regions));
        }
    }
}
```

关键函数：
- `generate_puzzle_piece_candidates()`: 对每个 puzzle_piece 格生成 8 朝向候选
- `enumerate_combinations()`: 笛卡尔积 + 去重叠
- `solve_remaining_rose()`: 在缩小后的网格上运行标准 region_match

### 3.5 homogeneous 扩展

对于 puzzle_piece + homogeneous 组合（1098/1099/1100）：Gemini 边要求两侧区域同形。
若一侧是 puzzle_piece 区域（形状已知），另一侧也必须同形。这进一步约束了另一侧的候选。

### 3.6 拼块信息在其他伴生规则中的利用

拼块形状确定后，可在候选筛选阶段预检查以下伴生规则，不需要留到最终验收：

| 伴生规则 | 可利用的约束 |
|---|---|
| `area` | 区域面积 = area 数字值 → 放置面积必须匹配 |
| `range` | 区域面积 ∈ [min, max] → 过滤不符合的放置 |
| `precise` | 所有区域面积 = precise 值 → 过滤 |
| `solitary` | 每区域恰好 1 个 clue → 过滤 clue 数量 ≠ 1 的放置 |
| `homogeneous` | Gemini 边两侧同形 → puzzle_piece 形状传递到相邻区域 |
| `heterogeneous` | Gemini 边两侧异形 → 相邻区域**不能**是 puzzle_piece 形状 |
| `fence` | 过滤跨 fence 边界的放置 |
| `block` / `non_block` | 过滤不满足矩形/非矩形约束的放置 |
| `different` | 所有区域形状互异 → puzzle_piece 形状不能重复出现 |
| `same` | 所有区域形状相同 → 所有区域必须匹配 puzzle_piece 形状 |

---

## 4. 搜索空间缩减估算

以 0732 为例：

| 步骤 | 当前 (BFS) | 优化后 |
|------|-----------|--------|
| 区域 A 候选 | 从 3 个 seed BFS, ~20000 候选 | **8 个朝向** |
| 有效候选 | BFS 中仅少数满足 puzzle_piece 形状 | **≤8 个**（符号过滤后可能 1-2 个） |
| 区域 B 候选 | BFS 从另 3 个 seed, ~20000 候选 | BFS 在 40 格残局上, ~几百候选 |
| 总搜索空间 | ~20000 × 20000 = 4 亿 | **~1 × 几百 ≈ 几百** |

缩减比例: **~10⁶ 倍**。

---

## 5. 实施计划

### Phase 1: 纯 puzzle_piece + rose_window（0732, 1096, 0225, 0226, 0224）

1. 实现 `generate_puzzle_piece_candidates()` — 生成 8 朝向候选并过滤
2. 修改 `region_match` 入口，跳过 `return None`，走 puzzle_piece 路径
3. 对剩余区域调用现有 BFS 逻辑
4. 测试 0732（最简单的 7×7, 1 个 puzzle_piece, M=2）

### Phase 2: puzzle_piece + area/range + rose_window（0344r, 1343）

1. 在候选筛选中加入面积约束
2. area: 区域面积必须等于 area 值（含 area 数字格）
3. range: 区域面积 ∈ [min, max]

### Phase 3: puzzle_piece + homogeneous + rose_window（1098, 1099, 1100）

1. 在候选筛选中加入 homogeneous 约束
2. Gemini 边两侧区域必须同形 → puzzle_piece 形状传递

### Phase 4: shape_pool 通用化

1. 将 puzzle_piece 逻辑扩展为 shape_pool 逻辑
2. shape_pool 中的每个形状都可作为「确定形状」的候选生成依据

---

## 6. 风险与注意事项

- **多 puzzle_piece 格的一致性**: 同一区域若有多个 puzzle_piece 格，它们必须指向同一形状（形状一致性），且各自锚点放置必须自洽
- **与非 puzzle_piece 区域的交互**: 确定了 puzzle_piece 区域位置后，剩余格可能不连通 → 需要 BFS 检查
- **homogeneous 传递链**: A-B-Gemini, B-C-Gemini → A、B、C 三者必须同形。若 A 是 puzzle_piece，则 B 和 C 也是同形
- **符号计数**: puzzle_piece 区域消耗了符号后，剩余区域的符号需求随之调整

---

## 7. 与其他优化的关系

- 与原型 #5（玫瑰二部图）**互补**: 若 K=2，先用二部图匹配配对符号，再放置 puzzle_piece 形状
- 与原型 #2（Bellman-Ford）**正交**: BF 解决面积约束传播，与形状放置无关
- 可视为 Shape Bank 约束的**特例路径**: puzzle_piece 本质是「单形状、强锚定的 shape_pool」

---

## 8. 实施结果（方向 4：rose 预钉 shape_pattern 区域，2026-08-08，分支 `rose-pp-pin`）

### 已实现

在 `rsolver/src/solver/rose/puzzle_piece_pin.rs` 新增预钉模块，`rose/mod.rs::solve_rose` 加 puzzle_piece 预钉分支，解除 `region_match.rs:285-291` 的 puzzle_piece/shape_pool 硬禁令。

**算法**（详见 `docs/rust-solver/07-rose求解器.md`）：
1. `enumerate_pin_candidates`：对每个 shape_pattern 格，枚举 pattern 的 dihedral 变体（≤8）× 合法放置（含锚点、全在网格、不压 blocked、不跨预画边界），符号约束过滤（per-type 计数相等）。
2. `enumerate_pin_assignments`：多锚点笛卡尔积（互不重叠 + 余数平衡）。
3. `solve_rose_with_pin`：缩减 all_positions + 算 m' → region_match → 合并预钉区域 → accept_if_valid。m'=1 快速路径（剩余格单连通分量直接成区域，避开 region_match 候选截断）。
4. **region_match 种子收集修复**：seeds/all_seed_cells 改为只从 `all_positions` 收集（原从全盘 puzzle.cells），使预钉移除符号格后种子数自动 = m'。

### 数据画像修正

调研文档原假设"171 题、backtrack 不理解拼块约束"。实际基准（cd40cab）数据：
- 171 题：**158 PASS（全 via aog）/ 13 FAIL**。backtrack **0 次触发**（印证 [[rsolver-review-findings-disproven]]，backtrack 在官方语料从不运行）。
- aog 原生支持 puzzle_piece（`AREA_SHAPE_INDEX_BIT` 增量检查），解出 158 道。
- 13 FAIL 聚类：4 道 puzzle_piece+rose_window（0732/1098/1099/1100）、3 道 puzzle_piece+brick+ring（OOM）、2 道 +watchtower、4 道其他。
- 方向 1（改 backtrack）无意义——backtrack 不跑。方向 4（rose 解禁）是正确靶点。

### 收益（puzzle_piece 子集基准，40s timeout，8 并发）

- official puzzle_piece：**159/171（基线 158）→ +1 PASS = 0732**，由 `rose` 解出（3005ms）。
- **0 回归**（无 PASS→FAIL），1 新解出（0732 FAIL→PASS）。
- 非官方语料另 +1 rose 解出。
- homogeneous 伴生 3 道（1098/1099/1100）：预钉 + rose 求剩余，剩余区域碰巧同形则 validate 通过（额外收益），否则拒绝零回归。实测未解出（靠 validate 兜底）。

### 关键技术点

- **shape_pattern 是 dihedral 类**（validate.rs:181-191 比对 `dihedral_key(&region.cells)` vs `dihedral_key(pat)`）——预钉需枚举 dihedral 变体放置，非唯一。0732：2 变体 × 7 放置 = 14 候选，符号约束过滤后 1 个 = 官方解。
- **rose_m 语义**：m = 全盘每类符号数。预钉后 m' = 剩余每类符号数（须相等，否则非法放置）。
- **region_match 候选截断**（CANDIDATE_CAP=20000）：m'=1 时大区域候选被截断 → m'=1 快速路径绕过。
