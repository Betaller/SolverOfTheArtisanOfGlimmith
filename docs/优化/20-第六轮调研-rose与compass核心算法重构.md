# 20 · 第六轮调研：rose 与 compass 核心算法重构

> 状态：**已完成**（2026-08-14，2 个 agent 并行调研 + 参考求解器实测）。
> 背景：rose_window（55 道）与 compass（41 道）是 edge_csp 两迭代落地后**最大的两个剩余 FAIL 簇**，
> 且都存在「参考求解器有未被本项目吸收的范式」。本轮深挖二者的核心算法重构方向。
> 关联：`docs/优化/18-第五轮调研`（rose 伴生剪枝债 + edge_csp 第二迭代）、`19-类型题专用求解器方向`
> （same/different/mixed）、`docs/rust-solver/11-edge-csp求解器.md`。
> 数据源：`results/tmp/20260814_edgecsp-full.jsonl`（186 FAIL，rose 55 / compass 41）。

---

## 0. 一句话结论

**rose 和 compass 的解法收敛到同一个宿主——edge_csp 的边传播范式。**

1. **rose**：参考 `pair.rs` 的「玫瑰对 SAME/DIFF 二分 + 边传播」在 6 道纯 rose 上有 **4/6 秒解级
   实测优势**（0213/0213nopad/0804/1433 均 0.0s~0.4s vs 本项目 40s timeout），且伴生规则
   （ring/brick/fence/compass）天然在边传播里处理（**一石二鸟，R1 都不用单独写**）。唯一例外
   1434（15×15、7 片、5 类型）参考也解不动。
2. **compass**：edge_csp 第二迭代只做了「方向计数 + bbox」（参考 L2 的一半），**缺的「桥/网关
   强制」是最高 ROI 的最小改动**，一次解锁 compass+ring/brick(4) + compass+watchtower(5) + 大
   compass 题收敛；纯 compass 还需「组件合并枚举」+ preempt。
3. **共同结论**：不是「给 rose/compass 各写一个独立求解器」，而是**把 rose 符号/pair 分支/
   compass 桥网关/放置枚举这些传播器接进已具备三态边基础设施的 edge_csp**。

---

## 1. rose 核心算法重构（55 道）

### 1.1 范式对比（含实测）

| 维度 | 本项目 region_match（BFS 枚举） | 参考 pair.rs（对分支 + 边传播） |
|---|---|---|
| 状态 | `CellSet` 位集，显式枚举「含 seed 的连通子集」 | 三态边数组，**不枚举区域子集** |
| 搜索树 | #连通子集 × #面积组合 × MRV（连通子集随格数指数） | 边 DFS + 对分支，传播器压小分支 |
| 内存 | CellSet + HashSet ≈ 64-104B/状态，9 seed × 2M = 1-2GB | O(边数+组件数)，与格数线性 |
| 伴生规则 | 搜索中完全不看，最后 validate 验一次 | 同一 propagate() 不动点并列（bricky_loopy/palisade/compass） |

**实测（Agent 用自写 JSON→aog 转换器喂参考 binary）**：

| 题 | 网格/可填 | 符号 | 本项目 | 参考求解器 |
|---|---|---|---|---|
| 0213 | 8×6 / 42 | 3 符号×2 | 40s timeout | **0.0s** |
| 0213nopad | 7×6 / 42 | 3 符号×2 | 40s timeout | **0.0s** |
| 0804 | 8×8 / 64 | 2 符号×9 | 40s timeout | **0.0s** |
| 1433 | 8×10 / 80 | 2 符号×4 | 40s timeout | **0.4s** |
| 1434 | 15×15 / 172 | 5 符号×7 | 40s timeout | **>90s 卡死**（257/292 未知） |
| C4-2 | 10×11 / 54 | 3 符号×6 | 40s timeout | 0.2s「No solution」 |

**结论**：参考范式从根本上避免候选爆炸——从不枚举连通子集。4/6 秒解、1 快速证无解；仅 1434
（片大 + 每类型 7 格 + 53 blocked + 仅 1 预画边）对参考也难（`propagate_parity` 只在「每类型恰 2
格」或「2 片」时启用，`propagation/rose.rs:265-288` 注释明确 bipartite 限制）。

### 1.2 失败模式判定（debug 实证 0804）

`AOG_DEBUG=1` 铁证：9 个 seed 全部撞 `CANDIDATE_CAP=20000`、面积组合撞 `MAX_COMBOS=50000`、
`match_regions_mrv` 在 50000×20000 空间每个 combo 给 1s → 预算烧光。**判定：候选数爆炸 +
精确覆盖空间大，二者叠加；不是 rose_growth 贪心找不到。**

### 1.3 rose+compass 9 道

关键发现：参考 `select_compass_branches_flat` 有硬门禁（`edges.rs:437-451`）：
```rust
if search_depth <= 3 && has_compass_clue && rose_bits_all == 0 && curr_unknown <= 80
```
**即 compass 平铺分支只服务 compass-only，在 rose 题里从不触发。** rose+compass 在参考里走的是
「rose 对分支 + compass 传播」（`propagate_compass_in_components` 零方向/方向计数/bbox/不兼容对）。
参考自带 sample `5x5-compass-rose-watch.txt` 实测 0.3s 解出。本项目 rose 求解器完全不看 compass。

### 1.4 rose+伴生 15 道：一石二鸟成立

伴生规则在参考范式里全是边/顶点级约束，天然在边传播里处理，**不需要单独写 R1**：

| 伴生规则 | 本质 | 参考传播器 |
|---|---|---|
| ring/brick | 顶点边界度 | bricky_loopy |
| watchtower | 顶点相接区域数 | vertex_edge_parity + watchtower |
| fence | 格四周 4 边 cut 模式 | palisade（edge_csp 已移植） |
| compass | 方向格计数 | compass_in_components |

**所以「候选枚举→对分支+边传播」同时解决候选爆炸（§1.1）和伴生缺口（R1 都不用单独写）。**
唯一例外：watchtower 传播 edge_csp 尚未落地（`prop.rs:49-90` 循环只有 bricky_loopy/compass/
palisade/area_bounds），rose+watchtower 题还需补 `propagate_watchtower` + `vertex_edge_parity`。

### 1.5 rose 方向路线图

| 阶段 | 内容 | 工作量 | 收益 | 风险 |
|---|---|---|---|---|
| **P1（立即还债）** | R1 伴生剪枝 + R2 多解兜底（`match_regions_mrv` 加 ring/brick/watchtower/fence/compass 增量检查 + `accept_if_valid` 失败转 MRV 回溯） | ~180 行 | +15~20（rose+伴生/compass） | 低（纯剪枝） |
| **P2（范式迁移）** | 把 rose 符号 + `exact_piece_count` + ParityUF + `propagate_parity` + `pair.rs`（select_rose_pair/branch_on_pair）+ 接线放行接进 edge_csp | ~1500-2000 行 | +20~30（含纯 rose 4 道） | 中 |
| **P3（补强）** | dual（Tarjan 桥）+ watchtower 传播 | ~1000 行 | 攻 1434 + rose+watchtower | 中高 |

**关键取舍**：1434 参考范式也解不动，方案 B 不是万能——「片大 + 每类型格数>2 + 复杂外形」题
parity/dual 双双失效，仍需面积界或形状库（aog/pieces）。不要指望覆盖全部纯 rose。

---

## 2. compass 完整求解（41 道）

### 2.1 参考的五级机制

| 层 | 函数 | 位置 | 触发 |
|---|---|---|---|
| L1 零方向强制 | `propagate_compass` | compass.rs:6-59 | 每轮传播 |
| L2 方向计数+bbox+**桥/网关** | `propagate_compass_in_components` | area.rs:652-902 | 每轮传播 |
| L3 放置枚举取交集 | `propagate_compass_placement_enumeration` | area.rs:1742-2124 | 每轮（max_area≤12/≤16 comps/≤500 放置） |
| L4 线索放置多联骨牌 | `generate_compass_polyominoes` | clue_placements.rs:85-248 | 仅 hybrid（spec≥3） |
| L5 混合求解 | `solve_hybrid` | match_solver.rs:75-209 | 仅 area+compass（total_clue_area>0 且 compass≤8） |

**关键**：edge_csp 第二迭代的「方向计数」= 参考 L2 **去掉桥/网关那半截**（`edge_csp/prop.rs:913`
注释「Bridge/gateway forcing left to iteration 3」）。参考 L2 把方向计数和桥/网关捆在同一函数里，
拆开后大 compass 题停在半路不收敛。

### 2.2 compass+solitary 14 道

- **根因**：aog 自由形状库 OOM（无 shape_pool 不走短路、compass 1-2 方向不给面积上界、
  `B-CompassUB` 只在四方向全指定时收紧）。参考的 `generate_compass_polyominoes` **不是解药**——
  参考自己不用它解纯 compass（`solve_normal` 里 `total_clue_area==0` 直接边搜索），且 spec 1-2 时
  候选海量被 `MAX_COMPASS_PLACEMENTS=2000` 截断。
- **正确路径**：edge_csp 加 **solitary 传播**（每区域恰一个 clue 格）+ compass 方向计数 + 桥/网关。
  但 `solitary` 不在 edge_csp 的 SUPPORTED（`mod.rs:509-520`），被排他门控挡掉。
- **预计**：小网格（1017/1246/1259/1260 的 6-7×7）1-3 道；13×13+（0681/1258/0680/1080）短期无解。

### 2.3 纯 compass 4 道（0445/0469/1395b/1074/0374）

- **纯 compass 三道**（0445/0469/1395b 16×14）：compass 只给方向计数不给面积，方向计数+bbox 剪
  不动；且**纯 compass 区域可不含任何 compass 格**，pieces/DLX 线索锚点放置结构性不可解。
  参考对纯 compass 就是「边搜索 + 传播」。需 **preempt（扩到 compass）+ 桥/网关 + 放置枚举（组件合并版）**。
- **area+compass 两道**（1074/0374）才是参考 `solve_hybrid` 正主（area 数给面积 + compass 给方向）。

### 2.4 compass+watchtower / compass+ring-brick（9 道）

参考把 compass/watchtower/ring/brick 全实现成**同一三态边数组上的传播器**，在不动点里轮流跑，
交互就是边变量联动——**不需要放置枚举与顶点度传播的特殊交互**。本项目缺口是：compass 桥/网关
缺位 + watchtower 传播未做（只在 select_edge 评分）。

### 2.5 pieces 已有 compass 枚举的 bug

`rsolver/src/solver/pieces.rs:449-570` `generate_compass_polyominoes`/`compass_rec` 是参考
`clue_placements.rs:85-248` 的移植，但**语义不同**：
- 参考 `Option::map_or`（None=不限，绝不剪）：`compass.n.map_or(false, |v| counts[0] > v)`。
- pieces 用 `unwrap_or(0)`：`counts[0] > compass.up.unwrap_or(0)`——**未指定方向被当成「必须为 0」**，
  spec<4 会过度剪枝、少枚举（`all_satisfied` 同样把未指定方向当 0）。
- 输入层已确认 `-1→None`（`io.rs:125-130`），所以 bug 真实触发。

另：`has_constrained_compass`（`solver/mod.rs:241-256`）要求 spec≥3，41 道里大量 spec 1-2 题
pieces 根本不跑 compass 枚举。

### 2.6 compass 方向 ROI 排序

| 排序 | 项 | 覆盖 | 工作量 | 风险 |
|---|---|---|---|---|
| 1 | **compass 桥/网关强制**（compass.rs:71-381 + Tarjan 桥 ~45 行自包含） | compass+ring/brick(4)+watchtower(5)+大 compass 收敛 | 小-中 | 低 |
| 2 | **`is_edge_csp_preempt` 接入 + 扩到 compass**（mod.rs:538-547，现只 ring 且 dead_code） | 止血 1395b/大 compass 的 aog 先 OOM | 小 | 低 |
| 3 | **watchtower 顶点配置传播**（迭代三） | compass+watchtower(5) | 中 | 中高 |
| 4 | **compass 放置枚举（组件合并版）**（area.rs:1742-2124） | 纯 compass + area+compass | 中高 | 中 |
| 5 | **修 pieces `unwrap_or(0)` bug + 放宽路由门槛** | 卫生修复，spec=3 的 compass+area | 小 | 低 |
| 6 | aog shape cap 默认启用 + compass LB/UB | 止血不求解 | 极小 | 低 |

**核心判断**：方向计数不够，先上**桥/网关**（最高 ROI），再 **preempt**，再**放置枚举（组件合并版）**。
`solve_hybrid` 是 area+compass 的正解范式，但对纯 compass 三道不是——那三道要「边搜索 + 桥/网关 + 放置枚举」。

---

## 3. 统一结论：edge_csp 是 rose + compass 的共同宿主

| 方向 | 短期（低风险） | 中期（范式迁移） |
|---|---|---|
| **rose** | R1 伴生剪枝 + R2 多解兜底（+15~20） | rose 符号 + ParityUF + pair 分支接进 edge_csp（+20~30，含纯 rose 4 道） |
| **compass** | 桥/网关强制 + preempt 扩到 compass（+9~14） | watchtower 传播 + 放置枚举（组件合并版） |

**落地依赖清单**（跨两方向合并）：
1. `ParityUF`（~73 行，`third_party/aog/src/uf.rs`）——rose 奇偶 + watchtower 奇偶共用。
2. `exact_piece_count` 推导（~25 行）——rose m=每类型计数，dual/loop_closure 前置。
3. `pair.rs` 对分支（select_rose_pair / branch_on_pair / branch_pair_same / bfs_path，~614 行）。
4. compass 桥/网关强制（~45 行 Tarjan 桥自包含 + 网关逻辑）。
5. watchtower 顶点配置传播（~250 行）。
6. compass 放置枚举组件合并版（area.rs:1742-2124 移植）。
7. `is_edge_csp_capable` 放行 solitary/rose + `is_edge_csp_preempt` 接入并扩到 compass。

---

## 4. 与已有文档的关系

| 文档 | 关系 |
|---|---|
| `18-第五轮调研` | 本文是其「rose 核心算法」与「compass 完整求解」两个被推迟维度的展开 |
| `19-类型题专用求解器方向` | 本文印证「不是独立求解器，而是扩 edge_csp 传播」的总体判断 |
| `14-边变量CSP独立求解器方案` | edge_csp 从「8 规则边约束」扩展为「rose/compass 也走边传播」的统一宿主 |
| `10-专用求解器方案` | BoundaryCSP（边变量）早期设计，本文是其 rose/compass 维度的具体化 |

**本文的独特价值**：
1. **实测证据**：自写转换器把 6 道纯 rose 喂参考 binary，得到 4/6 秒解级的硬数据，证明「对分支+
   边传播」范式对本项目纯 rose 题的绝对优势。
2. **澄清 rose+compass 的接法**：参考 `select_compass_branches_flat` 有 `rose_bits_all==0` 硬门禁，
   不是 rose+compass 的解法；正确接法是 rose 对分支 + compass 传播。
3. **发现 pieces compass 枚举的 `unwrap_or(0)` bug**（未指定方向被当成 0，spec<4 过度剪枝）。
4. **定调**：rose 和 compass 的最大杠杆都不是「新独立求解器」，而是「把传播器接进 edge_csp」。

---

> **后续处理**：本轮为调研文档。落地时按 CLAUDE.md 软门禁，同步 `docs/rust-solver/` 对应篇 +
> `docs/official-puzzles-status.md`，跑通 pytest / cargo test / benchmark，归档 artifacts。
> 建议顺序：rose R1+R2（低风险还债）→ compass 桥/网关 + preempt（最高 ROI）→ rose 范式迁移（P2）。
