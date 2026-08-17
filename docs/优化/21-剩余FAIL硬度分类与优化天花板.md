# 21 · 剩余 FAIL 硬度分类与优化天花板（第七轮综合）

> 状态：**已完成**（2026-08-14，综合 rounds 18-20 + watchtower 补挖）。
> 背景：edge_csp 两迭代（PR#33+PR#37）落地后，剩余 ~186 FAIL。前几轮已分别深挖 OOM/rose/fence
> （round 18）、专用求解器（round 19）、rose+compass 核心（round 20）。本轮把三者 + watchtower
> 收敛成一张「硬度分类 → 优化方法 → 预计解出 → 天花板」的完整地图，回答「还剩下什么、能解多少、
> 按什么顺序做」。
> 数据源：`results/tmp/20260814_edgecsp-full.jsonl`（186 FAIL）。关联 `docs/优化/18/19/20`。

---

## 0. 一句话结论

剩余 186 道 FAIL 按**瓶颈规则**分为五档，可解性递减：**① 配置 bug（19 道 OOM，止血即解）→
② 传播缺口（~25 道，edge_csp 补传播）→ ③ 范式错配（rose 55 + fence 剩余，需范式迁移）→
④ 形状同一性（~21 道，ShapeMatchSolver）→ ⑤ 根本难（~40 道，短期无解）**。前四档预计合计
**+60~95 道**（去重保守 +50），优化后 PASS 从 ~1072 推到 **~1120-1160**，天花板受限于第 ⑤ 档
（片大/复杂外形/组合爆炸，参考求解器同样解不动）。**最大杠杆仍是 rose（+35~50）与 compass
桥网关（+9~14），最小代价是 OOM 止血 + rose 还债（R1+R2）。**

---

## 1. 剩余 FAIL 全景

### 1.1 失败模式

| 模式 | 题数 | 性质 |
|---|---|---|
| No solution（搜索不完备） | ~117 | 剪枝不足 / 范式错配 |
| timeout（40s 超时） | ~45 | 搜索空间大 |
| OOM（exit -9） | 19 | **配置 bug（非算法）** |

### 1.2 规则画像（按题去重，重叠严重）

rose_window 55 > fence 42 ≈ compass 41 ≈ ring 40 > watchtower 35 > area 22 ≈ brick 22 >
solitary 19 > non_block 15 > homogeneous 13 ≈ different 13 ≈ difference 13 ≈ range 13 >
puzzle_piece 12 ≈ inequality 12 ≈ differentiation 11 > heterogeneous 7 > mixed 4 ≈ same 4 >
shape_pool 3 ≈ precise 3 > block 1。

**关键洞察**：单规则 FAIL 极少（纯 rose 6、纯 watchtower 3、纯 compass 4），**绝大多数是
2-4 规则组合**，瓶颈往往是「其中一个规则缺传播/范式错配」而非「多条规则都不懂」。

---

## 2. 五档硬度分类

### 档 ①：配置 bug（19 道 OOM）—— 止血即解，非算法

- **根因**（round 18）：`DEFAULT_SHAPE_CAP=0`（aog 形状库默认无上限）+ aog 先跑抢先 OOM +
  `is_edge_csp_preempt` 已写未接线。19 道全在 aog 自由形状库（`search.rs:263` 单调增长）。
- **优化方法**：`AOG_SHAPE_CAP` 默认启用 + preempt 接线（**扩到 compass**，不只 ring）。
- **预计**：19 OOM → 5 道 edge_csp-capable（0977/0631/1110/0491/1395b）可能被接住 + 14 道
  优雅超时（仍 FAIL 但不再 exit -9）。**净解出 0~5 道，但消除 OOM 类崩溃**。

### 档 ②：传播缺口（edge_csp 补传播即可，~25 道）

这些题 edge_csp 已 capable（规则 ⊆ SUPPORTED）但缺某个传播器：

| 缺口 | 覆盖题 | 预计解出 | 工作量 | 风险 |
|---|---|---|---|---|
| **compass 桥/网关强制**（`compass.rs:71-381` + Tarjan 桥 ~45 行自包含） | compass+ring/brick(4) + compass+watchtower(5) + 大 compass 收敛 | +9~14 | 小-中 | 低 |
| **watchtower 传播**（组件 ID pass + 边 pass 环/树双触碰 + 顶点边奇偶 ParityUF + 配置探测） | compass+watchtower(5) + 部分 rose+watchtower | +4~8 | 中-高（~1000 行，双触碰语义微妙） | 高 |
| **solitary 传播**（每区恰一 clue 格） | compass+solitary 小网格(1017/1246/1259/1260) | +1~3 | 小 | 低 |
| **differentiation(size_separation) + boxy/non_boxy**（round 19） | 纯 differentiation/non_block 子集 | +3~6 | 小 | 低 |
| **gemini⇒尺寸相等 + delta_gemini 顶点交互**（round 19） | heterogeneous/homogeneous 尺寸耦合系(0586/0619/0742/0866/0965/0990) | +6~8 | 中（~120 行） | 低 |

> watchtower 双触碰语义（round 20 补挖）：环（N=4,E=4）`pieces=max(1,k)`，树（N=2..3）`pieces=1+k`；
> value=2/3/4 允许双触碰（k 可超最小值），仅 value=1 精确强制；另有 `apply_watchtower_value_one_optimization`
> （`mod.rs:423`，value=1 ⇒ 四边全 Uncut）。本项目 edge_csp 目前 watchtower 只在 `select_edge`
> 评分（`mod.rs:373-380`），**零传播**。

### 档 ③：范式错配（rose 55 + fence 剩余，需范式迁移）

| 子档 | 覆盖题 | 优化方法 | 预计解出 | 工作量 | 风险 |
|---|---|---|---|---|---|
| rose 短期还债 | rose+伴生/compass ~20 | **R1 伴生剪枝 + R2 多解兜底**（`match_regions_mrv` 加 ring/brick/watchtower/fence/compass 增量 + validate 失败转回溯） | +15~20 | ~180 行 | 低 |
| rose 范式迁移 | 纯 rose 4 + rose+compass 多数 | rose 符号 + ParityUF + pair 分支接进 edge_csp（round 20，实测 4/6 秒解） | +20~30 | ~1500-2000 行 | 中 |
| fence 剩余 | fence+ring/brick/compass | palisade 传播入 edge_csp + fence 路由（round 18 F1，PR#37 已落地 fence 4 道） | +8~12（去重后 +4~8） | 中 | 低 |

### 档 ④：形状同一性（~21 道，ShapeMatchSolver）

| 规则 | 优化方法 | 预计解出 | 工作量 | 风险 |
|---|---|---|---|---|
| same | 移植参考 `solve_match`（除数分解 + 单形状 DLX + **2 片耦合 ~250 行**） | 3/4 | 中高（~400 行） | 低 |
| different（有面积界） | DLX 辅列「每形状键 ≤1」（`add_secondary_column`） | 3-4 | 中（~250 行） | 中 |
| mixed（固定形状库） | 形状库 DLX + 相邻异形后验 | 1-2 | 低-中 | 中 |

### 档 ⑤：根本难（~40 道，短期无解）

| 特征 | 典型题 | 为何难 |
|---|---|---|
| 片大 + 每类型格数>2 + 复杂外形 | 1434（15×15 5符号×7） | 参考 parity/dual 双双失效（round 20 实测 >90s） |
| compass+solitary 大网格 | 0681/1258/0680/1080（13×13+） | 形状库 OOM + 无面积上界 + 组合爆炸 |
| 多规则组合爆炸 | area+differentiation+solitary、brick+fence+ring+rose 等 | 每条规则都需传播，任一缺位就爆炸 |

> 这一档是「参考求解器也解不动」的题——不是本项目能力短板，是问题本身对当前边传播+形状放置
> 范式的固有难度。进一步突破需要新范式（SAT/ILP/学习启发式），超出当前迭代范围。

---

## 3. 优化天花板估算

| 档 | FAIL 数 | 预计解出（去重后） | 备注 |
|---|---|---|---|
| ① 配置 bug | 19 | +0~5 | 止血为主，5 道可能被 edge_csp 接住 |
| ② 传播缺口 | ~25 | +15~25 | compass 桥网关 + watchtower + solitary + differentiation/boxy + gemini |
| ③ 范式错配 | rose 55 + fence 剩余 | +35~50 | R1+R2（+15~20）+ rose 迁移（+20~30，与 R1 部分重叠） |
| ④ 形状同一性 | ~21 | +7~10 | ShapeMatchSolver |
| ⑤ 根本难 | ~40 | 0 | 短期无解 |
| **合计** | **186** | **+60~95（保守 +50）** | 重叠去重后 |

**结论**：优化后 PASS 从 ~1072 推到 **~1120-1160 / 1258**（+50~90），剩余 ~100-135 FAIL 集中在
档 ⑤（根本难）与档 ③ 的硬骨头（1434 类）。**这是当前「边传播 + 形状放置」范式组合的天花板**。

---

## 4. 完整优先级路线图

| 阶段 | 内容 | 预计解出 | 工作量 | 风险 | 依据 |
|---|---|---|---|---|---|
| **P0 止血** | AOG_SHAPE_CAP 默认启用 + preempt 接线（扩 compass）+ edge_csp 内部叶节点验证 | +0~5（消 19 OOM） | 极小 | 低 | round 18 §2.1 |
| **P1 还债** | rose R1+R2（伴生剪枝 + 多解兜底） | +15~20 | ~180 行 | 低 | round 18/20 |
| **P2 最高 ROI** | compass 桥/网关强制 + preempt | +9~14 | 小-中 | 低 | round 20 |
| **P3 范式迁移** | rose 符号 + ParityUF + pair 分支接进 edge_csp | +20~30 | ~1500-2000 行 | 中 | round 20 |
| **P4 传播补齐** | watchtower 传播 + differentiation/boxy + gemini 尺寸 + solitary | +15~25 | 中-高 | 中-高 | round 18/19/20 |
| **P5 专用求解器** | ShapeMatchSolver（same/different/mixed）+ aog NonBlockFilter | +7~18 | 中-高 | 中 | round 19 |
| **P6 卫生修复** | 修 pieces compass `unwrap_or(0)` bug + 放宽路由门槛 | 小 | 小 | 低 | round 20 §2.5 |

> 建议顺序：**P0 → P1 → P2 → P3 → P4 → P5 → P6**。P0/P1 是「已知根因的还债」，风险最低、
> 立竿见影；P2 是最小改动最大覆盖；P3 是最大单笔收益但工作量大；P5 是独立求解器（不回归主力）。

---

## 5. 与前几轮的关系

| 文档 | 本文如何收敛 |
|---|---|
| `18-第五轮调研` | 档 ①（OOM 配置）+ 档 ③（rose/fence）+ P0/P1 的直接来源 |
| `19-类型题专用求解器方向` | 档 ④（形状同一性）+ 档 ② 的 gemini/non_block 部分 |
| `20-rose与compass核心算法重构` | 档 ③（rose 范式迁移）+ 档 ②（compass 桥网关） |

**本文的独特价值**：
1. 把三轮分散的「优化点」收敛成**五档硬度分类 + 完整优先级路线图**，回答「还剩下什么、按什么
   顺序做、能到多少」这一 meta 问题。
2. 给出**优化天花板**（~1120-1160 PASS）与「根本难」档（~40 道），避免对不可解题过度投入。
3. 补挖 watchtower 双触碰语义（环 `max(1,k)`/树 `1+k`/value=1 精确），这是 rounds 18-20 唯一
   未展开到位的传播器。

---

> **后续处理**：本轮为综合文档。落地按 §4 路线图分阶段执行，每阶段遵循 CLAUDE.md 软门禁：
> 同步 `docs/rust-solver/` 对应篇 + `docs/official-puzzles-status.md`，跑通 pytest / cargo test /
> benchmark，归档 artifacts。建议从 P0（止血）+ P1（rose 还债）起步。
