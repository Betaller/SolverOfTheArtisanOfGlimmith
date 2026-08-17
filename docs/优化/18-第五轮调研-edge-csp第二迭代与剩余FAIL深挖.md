# 第五轮调研：edge-csp 第二迭代与剩余 FAIL 深挖

> 状态：**已完成**（2026-08-14，3 个 agent 并行深挖）。
> 背景：edge_csp 边变量 CSP 求解器**第一迭代已落地**（`edge-csp-solver` 分支，见
> `docs/rust-solver/11-edge-csp求解器.md`），覆盖 ring/brick/area/precise/range/
> inequality/difference。本轮调研目标：(1) 量化第一迭代收益并确定剩余 FAIL 基线；
> (2) 深挖 edge_csp 第二迭代（compass/watchtower/differentiation/boxy/fence）的移植方案；
> (3) 深挖剩余 FAIL 最大的两个簇 rose_window / fence 的根因；(4) 扫描第五轮新优化角度。
> 关联：`docs/优化/14-边变量CSP独立求解器方案.md`（设计）、`docs/rust-solver/11-edge-csp求解器.md`
> （实现）、前四轮调研 `docs/优化/11~17`。

---

## 0. 一句话结论

第五轮深挖出三个「最高杠杆」发现，全部是**已知根因/已知方案的还债**，而非新算法：

1. **19 道 OOM 是配置问题不是算法问题**：`DEFAULT_SHAPE_CAP=0`（aog 形状库默认无上限）+
   aog 先跑抢先 OOM + `is_edge_csp_preempt` 已写好未接线——三者叠加把 19 道 ring/brick/fence
   自由形状题送进 OOM-killer。把 cap 默认启用 + 接线 preempt 即可一次性转「5 道可能被
   edge_csp 接住 + 14 道优雅超时」（白捡，docs 11-17 都漏了这条最直接杠杆）。
2. **rose_window 簇（50 题）的最大缺口是「伴生规则剪枝」从未落地**（第二轮已定位、已给方案
   S1，但一直没写）：rose 搜索只看自身约束、伴生规则仅最后 `validate` 验一次即弃。补上
   R1 伴生剪枝 + R2 多解兜底预计 **+18~25 题**，且全部复用 backtrack 已实现并验证的检查逻辑。
3. **edge_csp 第二迭代的杠杆在 compass + watchtower 传播**，共同前提是「内部叶节点验证」
   （当前 `backtrack_edges` 无条件存首解，方向计数错误的中间解会烧光整段预算）。

fence 簇（38 题）的架构级路线是把 **palisade 传播移植进 edge_csp 并把 fence 题路由进来**
（cell-variable backtrack 对 fence 天然范式错配，backtrack 对 fence 子集贡献 0 题）。

---

## 1. 当前基线（第五轮起点）

### 1.1 基准口径

- **PRE（edge_csp 前基线）**：`results/bench/20260809_4df037f_full.jsonl`，全量 1258 题，
  **PASS 1054 / FAIL 204**。
- **POST（edge_csp 第一迭代）**：`results/tmp/20260814_edgecsp-full.jsonl`，全量跑完
  **PASS ~1072 / FAIL ~181**（三个 agent 在不同时刻读到的快照为 162/163/181，绝对数随并行
  负载 ±5 浮动；结构结论一致）。

> 说明：`edge-csp-solver` 分支除 edge_csp 模块外，还混有 N1（backtrack HashMap→Vec/BTreeMap
> 消非确定，PR #29）、N2/N3/N6（aog deadline 盲区，PR #30）等未提交改动。故 POST−PRE 是
> **合并效应**，非 edge_csp 单独贡献。edge_csp 直接归因的 ~11 道见
> `docs/rust-solver/11-edge-csp求解器.md` §5（0637/0638/1134/0979/1404/0507/0592/1400/0894/1382/1411）。

### 1.2 规则级 FAIL 对比（PRE → POST，按题去重）

| 规则 | PRE | POST | 变化 | 规则 | PRE | POST | 变化 |
|---|---|---|---|---|---|---|---|
| rose_window | 59 | ~55 | −4 | non_block | 16 | 15 | −1 |
| ring | 45 | ~40 | −5 | homogeneous | 15 | 13 | −2 |
| compass | 43 | ~41 | −2 | differentiation | 14 | 11 | −3 |
| fence | 43 | ~42 | −1 | different | 15 | 13 | −2 |
| watchtower | 35 | ~33 | −2 | puzzle_piece | 12 | 12 | 0 |
| difference | 26 | 14 | **−12** | heterogeneous | 8 | 7 | −1 |
| area | 26 | ~22 | −4 | precise | 7 | 3 | −4 |
| inequality | 23 | 13 | **−10** | block | 5 | 1 | −4 |
| solitary | 23 | 19 | −4 | same | 4 | 4 | 0 |
| brick | 22 | ~22 | 0 | mixed | 4 | 4 | 0 |
| range | 20 | 14 | −6 | shape_pool | 3 | 3 | 0 |

**结论**：第一迭代把 **difference(−12)/inequality(−10)** 大幅清掉（edge_csp 核心优势规则），
但 **rose_window / fence / ring / compass 仍是最大簇**——正是第二迭代 + 还债的方向。

### 1.3 失败模式分布（POST 全量）

| 失败模式 | 题数 |
|---|---|
| No solution（搜索不完备 / 剪枝不足） | ~117 |
| timeout（40s 超时） | ~45 |
| OOM（exit -9） | **19** |

---

## 2. 三大根因与最高杠杆发现

### 2.1 OOM（19 道）根因 = 配置问题，非算法问题（最高杠杆白捡）

**19 道 OOM 全部发生在 aog 自由形状库枚举**，无一在 edge_csp/rose/pieces/backtrack。定位依据：

1. **路由顺序**（`rsolver/src/solver/mod.rs:72-118`）：aog 永远最先跑。19 道 OOM 题没有一道
   含 rose_window → `is_rose_capable`=false → aog 拿完整 40s 预算；edge_csp/pieces/backtrack
   都在 aog 返回 `None` 之后才跑——aog 被 OOM-killer SIGKILL 时**从不返回**，后续求解器从未运行。
2. **aog 内存热点**：`AoGCore::shapes` 形状库在 `search.rs:263` 每次自由形状放置时单调增长、
   永不收缩。上限 `shape_cap` 由 `AOG_SHAPE_CAP` env 或 `DEFAULT_SHAPE_CAP` 决定，
   而 **`types.rs:71` 的 `DEFAULT_SHAPE_CAP = 0`（关闭）**，`rust_solver.py:157-165` 只透传
   `RSOLVER_TIMEOUT_MS`、**未设 `AOG_SHAPE_CAP`** → 全量跑时形状库无上限。

**逐题归类**（19 道里 edge_csp 是否可接手）：

| 类别 | 题号 | 为何没进 edge_csp |
|---|---|---|
| edge_csp-capable 但被 aog OOM 堵死 | **0977**(brick+ring+area)、**0631/1110**(brick+ring+compass)、**0491**(watchtower)、**1395b**(compass 16×14) | aog 先跑先 OOM |
| 含 fence/solitary/puzzle_piece/different/differentiation 被 `is_edge_csp_capable` 排除 | 0439、0606、0976、1215、0834、0978、0969、0980、1373/1374b/1375/1378、1060、0629 | SUPPORTED 无这些规则 |

**两项白捡修复**（docs 11-17 都漏了）：
- **#1 启用 AOG_SHAPE_CAP**：`DEFAULT_SHAPE_CAP` 0→非 0（或 benchmark 显式传 env）。shape cap
  已落地（doc 15/16），但「默认 0 = 全量跑未启用」这个 gap 是新的。19 OOM 一次性转优雅超时。
- **#2 接线 `is_edge_csp_preempt`**：`edge_csp/mod.rs:522-535` 已写好、注释「reserved iteration 2」
  未接线。接线后 5 道 edge_csp-capable 题在 aog 之前先跑 edge_csp，绕过 aog OOM。

### 2.2 rose_window 簇：伴生规则剪枝债（还债 = 最大确定性收益）

见 §4。核心：第二轮已定位「rose 只管自身约束、伴生规则最后 validate 兜底验一次即弃」，
方案 S1（伴生规则剪枝）~150 行**从未落地**。这是把「已知根因 + 已知方案」的债还掉，风险最低。

### 2.3 fence 簇：cell-variable backtrack 范式错配

见 §5。核心：fence 是**边变量约束**，参考求解器用 `propagate_palisade_constraints` 主动枚举
兼容旋转、强制所有兼容旋转都一致的边；本项目 fence 是 backtrack cell-variable 的被动事后检查，
信息量天然少。铁证：**fence 子集 166 题中 128 PASS 全由 aog 解出，backtrack 贡献 0 题**。

---

## 3. edge_csp 第二迭代移植方案（参考 `third_party/aog`）

### 3.1 现状关键事实

- edge_csp 传播循环（`prop.rs:49-84`）只调 **3 个传播器**：`bricky_loopy`、`area_bounds`
  （含 inequality/diff）、probing。参考实现 `propagation/mod.rs:94-208` 有 12 个传播器。
- **compass 只传播面积界、不传播方向计数**（`prop.rs:210-270` 只算 min/max 面积）。
- **内部验证缺失**：`backtrack_edges` 在 `curr_unknown==0` 时无条件保存首解（`mod.rs:418-421`），
  `solve_edge_csp` 入口 `validate` 一次即弃（`mod.rs:466-478`）——方向计数错误的中间解会
  `None` + 整段预算烧光。**这是 compass/watchtower 收益的前置**。
- `is_edge_csp_capable`（`mod.rs:489-520`）SUPPORTED 只有 9 条规则；**fence/differentiation 的
  differentiation 已在 SUPPORTED 但零传播**（`adapter.rs:154` 的 `size_separation` flag 是死代码）。
- edge_csp **没有 piece/rose 概念**：`exact_piece_count`（dual.rs 前置）无来源，out of scope。

### 3.2 各传播器移植评估

| 传播器 | 核心算法 | 基础设施缺口 | 预计价值 | 风险 |
|---|---|---|---|---|
| **compass 方向计数** | 零方向强制 + 每组件数 N/S/E/W 格、at-limit Cut、单网关 Uncut、bbox 框外 Cut | 组件缓存已齐；缺 Tarjan 桥(~45行自包含 `compass.rs:386`) | **4-8**（compass ~41 FAIL） | 中（方向映射、bbox 过度强制已证伪过一版） |
| compass 放置枚举 | `max_area≤12` 时 DFS 枚举合法放置取交集 | 无 | 与方向计数重叠 | 高（框内格误判）→ 暂缓 |
| **watchtower 基础** | 组件 ID pass + 边 pass（环/树 + 双触碰语义） | 无新依赖 | **1-3**（watchtower ~33） | 高（双触碰语义微妙，需搬参考测试） |
| watchtower 增强 | ParityUF 顶点边奇偶 + 配置探测 | **ParityUF ~73 行**（`uf.rs` 未落地） | 与基础重叠 | 高 |
| **differentiation** | `area.rs:370` sealed 邻居尺寸集合 → 合并撞 forbidden 强制 Cut | 无（flag 已是死代码，补传播即可） | **3-6**（differentiation 13） | 低 |
| **boxy/non_boxy** | `area.rs:904` bbox 判断密封组件矩形性 | `GlobalRules` 补 boxy/non_boxy 字段 + adapter 映射 | **3-5**（block 2 + non_block 16） | 低 |
| dual.rs（Tarjan 桥） | 需 `exact_piece_count≥2` | **无 piece 概念** | 低 | — |
| palisade（fence） | 见 §5 | 见 §5 | 见 §5 | 低 |
| delta_gemini 顶点交互 | `delta_gemini.rs:13` gemini+delta 共线时正交边互斥 | 无 | 少数 heterogeneous/homogeneous 混排 | 低 |
| shape.rs（gemini/delta/mingle/mismatch） | 只在 sealed 时校验 canonical shape，非增量 | — | 低（本质需 pieces/aog 完整形状放置） | 中 → 跳过 |

### 3.3 第二迭代实施顺序建议（Agent 1）

1. **P0 内部叶节点验证**：改 `backtrack_edges` 保存逻辑（`mod.rs:418-421`）+ `solve_edge_csp`
   入口策略。compass/watchtower 一切收益的前提，不依赖任何新传播器。
2. **P1 compass 方向计数**（~300 行，先轻量层：零方向 + 方向计数 + bbox，跳过放置枚举与桥/网关）。
3. **P2 watchtower 基础传播**（~250 行，无新依赖）。
4. **P3/P5 differentiation + boxy/non_boxy**（代码量小，需扩 `is_edge_csp_capable` 门控）。
5. P4 watchtower 增强视 P2 实测再决定；P6 compass 放置枚举、dual、shape 暂缓/不做。

---

## 4. rose_window 簇根因与修复（50 题）

### 4.1 失败分类

**NOSOL 28 / timeout 22 / OOM 0**（历史 OOM 0882/0826/0838/0999 已被 `MAX_COMBOS` +
`VISITED_CAP` 转成优雅 timeout/NOSOL）。伴生规则画像：15 题伴 ring/fence/watchtower/compass、
7 题伴 same/different（**rose 求解器根本没被调度**）、6 题伴 non_block/brick/difference。

### 4.2 根因（证据链）

rose 求解器搜索中只看 rose_window 自身三个约束（连通性 / 每区域恰含每符号一次 / 预画边界），
**完全不看任何伴生规则**；伴生规则只在最后 `validate` 兜底、且只验一次，验不过就整体放弃、
不回溯重试：

1. 候选生成 `region_match.rs:53-175` 剪枝只有 `pre.contains`/`syms==all_required`/`MAX_CANDIDATE_CELLS`。
2. MRV 精确覆盖 `region_match.rs:548-633` 唯一一致性检查是 `check_boundaries_partial`（只查边界）。
3. `match_regions_mrv` 命中即 `return true`（613-626），只产出**一个** rose 合法划分。
4. `rose/mod.rs:73-79` `accept_if_valid` 对唯一划分验一次；`rose_growth` 同样只产出一个贪心划分。
5. `is_rose_capable`（`solver/mod.rs:225-234`）对含 same/different 的 rose 题直接返回 false
   → 7 题根本没进 rose 求解器。

### 4.3 修复方向（ROI 排序）

- **R1（最高 ROI）rose 伴生规则剪枝 S1**：在 `match_regions_mrv` 每次赋值后加增量伴生检查，
  复用 backtrack 已实现的三段：ring/brick 节点度下界（`backtrack.rs:1045-1084`）、watchtower
  已见区域数（`backtrack.rs:932-947`）、fence arm_count（`fence/check.rs:154-163`），外加 compass
  方向计数超界检查。~100-150 行，**预计 +15~20 题**，风险低（复用已验证逻辑，纯剪枝不产生假阳）。
- **R2 rose 多解兜底**：把 `validate` 失败作为 MRV 回溯信号（命中完整覆盖时先 `validate`，不过
  就 `return false` 继续找下一个覆盖）。~30 行，覆盖「第一个覆盖过不了伴生规则」一类。与 R1 互补。
- **R3 解除 same/different 门禁 + combo 惰性化**：(a) `is_rose_capable` 放开 same/different
  （在 MRV 加形状去重检查），+3~7 题；(b) `enum_area_combos_bounded` 静态 5 万截断改惰性流式
  （配合已有 `PER_COMBO_TIMEOUT_MS`），救 0882/0826 这类 timeout。

---

## 5. fence 簇根因与修复（38 题）

### 5.1 失败分类与根因

**NOSOL 29 / OOM 7 / timeout 2**。关键事实：**fence 子集 166 题中 128 PASS 全由 aog 解出，
backtrack 贡献 0 题**——`fence/check.rs` 这套挂在 backtrack 的守卫自落地以来没多解出任何一题。

`check_fence_patterns`（`fence/check.rs:123-175`）的两个弱点 + 一个更深错配：
- **触发太晚（主因）**：只对已赋值的 fence cell 求位（`132-134`），backtrack 逐格长区域，
  fence cell 边界「涌现」式确定，最强的一步（全定 dihedral 比对）要等四邻全定才 fire，太迟。
- **部分检查太弱（次因）**：arm_count 只卡 `true>k / false>4-k` 极端，对占大头的 2-arm 模式几乎不触发。
- **范式错配（根因）**：fence 是边变量约束，参考 `propagate_palisade_constraints`
  （`palisade.rs:34-136`）主动枚举兼容旋转、强制所有兼容旋转一致的边；本项目是 cell-variable
  被动事后检查，信息量天然少。

### 5.2 修复方向（ROI 排序）

- **F1（最高 ROI）palisade 传播进 edge_csp + fence 题路由进来**：edge_csp 已维护三态边 + 传播
  ring/brick，正是 palisade 天然宿主。移植 `palisade.rs:34-136`，`is_edge_csp_capable` SUPPORTED
  加 `fence`，让 fence+ring+brick（OOM 7 题）、fence+compass（1092）、fence+precise（0709）等
  边约束稠密题走 edge_csp。~200 行 + adapter 加 fence cell clue，**预计 +8~12 题 + 顺带消 7 OOM**。
  这是 fence 簇唯一有架构级收益的方向。
- **F2 强化 backtrack fence 守卫为「边位下推」**：在 `check_fence_patterns` 对 unknown 位做
  「枚举兼容 arm 集 → 所有兼容旋转都 cut/uncut 则视为已确定」的局部传播（palisade 单 cell 弱化版）。
  +2~4 题，风险低，但天花板有限（仍是 cell-variable）。
- **F3 fence 稠密题前置 edge_csp preempt**：对「fence 密度高 + 无 shape_pool/mixed」的题（如
  0439 纯 palisade 31 个 fence cell）在 aog 前路由到 edge_csp。与 F1 一起做收益最大。

---

## 6. 第五轮新优化点清单（统一优先级）

综合三个 agent，去重、调和分歧后（fence 入 edge_csp 是 Agent 2/3 一致结论、Agent 1 仅因
未考虑同步扩路由门控而低估），最终优先级表：

| 优先级 | 项 | 机制（一句话） | 预计受益 | 工作量 | 风险 |
|---|---|---|---|---|---|
| **P0** | **AOG_SHAPE_CAP 默认启用 + preempt 接线** | `DEFAULT_SHAPE_CAP` 0→非0；`is_edge_csp_preempt` 接线，edge_csp-capable 题在 aog 前先跑 | 19 OOM 转优雅超时，5 题可能被 edge_csp 接住 | 极小 | 低（shape cap 已验 16/21） |
| **P0** | **edge_csp 内部叶节点验证** | 叶节点 `validate` 通过才 save，无效继续搜 | 解锁 compass/watchtower 全部后续收益 | 小 | 低 |
| **P1** | **rose 伴生剪枝 R1 + 多解兜底 R2** | 复用 backtrack 已验逻辑做 MRV 增量剪枝；validate 失败转回溯 | **+18~25** | 中（~150 行） | 低 |
| **P1** | **compass 方向计数传播** | 零方向 + 每组件数方向格 + bbox | **+4~8** | 中（~300 行） | 中 |
| **P2** | **fence 入 edge_csp（palisade + 路由）** | 移植 `palisade.rs` + SUPPORTED 加 fence | **+8~12 + 消 7 OOM** | 中（~200 行） | 低 |
| **P2** | **watchtower 基础传播** | 组件 ID pass + 边 pass（环/树双触碰） | **+1~3** | 中（~250 行） | 高 |
| **P3** | **differentiation(size_separation) 传播** | 移植 `area.rs:370`（flag 已是死代码，补传播） | +3~6 | 小（~120 行） | 低 |
| **P3** | **boxy/non_boxy 传播 + 放行 block/non_block** | 移植 `area.rs:904` + SUPPORTED 加 block/non_block | +3~5 | 小（~100 行） | 低 |
| **P3** | **rose 解除 same/different 门禁 + combo 惰性化** | `is_rose_capable` 放开 + `enum_area_combos_bounded` 惰性流式 | +3~7 | 中 | 中 |
| **P4** | **watchtower 增强（ParityUF + 配置探测）** | 移植 `uf.rs` + `watchtower.rs:330/539` | 与基础重叠 | 中（+73 行 UF + 枚举） | 高 |
| **P4** | **delta_gemini 顶点交互 / same-area-groups** | 小项顺手 | 少数 | 小 | 低 |
| **暂缓** | compass 放置枚举 / dual.rs / shape.rs 全传播 / match_coupled | 收益低或需 piece/rose 前置 | — | — | — |

**一句话路线**：**P0（OOM 止血 + 内部验证）→ P1（rose 还债 + compass）→ P2（fence 入 edge_csp
+ watchtower）→ P3（differentiation/boxy/same-different 尾巴捡漏）→ P4（watchtower 增强）**。

**最高杠杆单一结论**：19 道 OOM 的根因是「`DEFAULT_SHAPE_CAP=0` 默认关闭 + aog 先跑 +
`is_edge_csp_preempt` 未接线」三者叠加——把 cap 默认打开 + 接线 preempt，即可把 19 道 OOM
一次性转为「5 道可能被 edge_csp 解出 + 14 道优雅超时」，这是 docs 11-17 都漏掉的最直接杠杆。

---

## 7. 结论与下一步

### 7.1 本轮三点定性

1. **不再是「找新算法」，而是「还债 + 补传播」**：第五轮的三大发现（OOM 配置、rose 伴生剪枝、
   fence 范式错配）都是前几轮已定位但未落地或未接线的项；edge_csp 第二迭代的 compass/watchtower
   传播则是第一迭代明确留白、参考实现现成的移植。**没有需要从零发明的新算法**。
2. **收益上限**：P0+P1+P2+P3 合计预计 **+35~60 题**（去重后保守 +30），可把 FAIL 从 ~181
   推到 ~120-150 区间。最大单项是 rose 还债（+18~25）与 fence 入 edge_csp（+8~12）。
3. **风险分层清晰**：P0/P1 复用已验证逻辑、纯剪枝/多解，风险最低；watchtower 双触碰语义与
   compass 放置枚举风险最高，需搬参考测试。

### 7.2 推荐实施顺序

1. **阶段 0（止血）**：AOG_SHAPE_CAP 默认启用 + `is_edge_csp_preempt` 接线 + edge_csp 内部叶节点验证。
2. **阶段 1（还债）**：rose 伴生剪枝 R1 + 多解兜底 R2。
3. **阶段 2（edge_csp 第二迭代主体）**：compass 方向计数 → watchtower 基础 → fence 入 edge_csp。
4. **阶段 3（尾巴）**：differentiation / boxy/non_boxy / same-different 门禁 / delta_gemini。
5. **阶段 4（增强，视实测）**：watchtower ParityUF+配置探测、compass 放置枚举。

---

> **后续处理**：本轮为调研文档。落地时按 CLAUDE.md 文档软门禁，同步 `docs/rust-solver/`
> 对应篇（尤其 `11-edge-csp求解器.md` §7 第二迭代）+ `docs/official-puzzles-status.md`，
> 跑通 pytest / cargo test / benchmark，归档 artifacts。全量基准跑完后用同一脚本回填 §1 精确数字。
