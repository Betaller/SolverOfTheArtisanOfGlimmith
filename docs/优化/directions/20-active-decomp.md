# 20 · 主动降维分治

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N5）｜ 来源：`docs/优化/24` §11.5
> 关联：[21-deduction-engine.md](21-deduction-engine.md) · [19-simd-flood.md](19-simd-flood.md)

## 1. 一句话
`11` §3.6 的降维分解依赖「已有确定边界分隔」。本方向更进一步：**主动推断边界**以最大化分解 —— 先用 compass 边界框、ring/brick 顶点度传播、watchtower 双触碰把尽可能多边「钉死」为 Boundary/NonBoundary，再把网格切成多个互不连通子网格分别求解后拼回。

## 2. 思想（为什么有效）
- 分治是指数级收益：16×16 切成两个 ~8×8，复杂度从 O(2^256) 降到 O(2^128 + 2^128)。这是本题**对抗搜索爆炸最强**的结构性手段之一（`11` §3.6 标为「✅ 高收益」）。
- 关键升级：`11` 的分解是**被动**的（有确定边界才分）；本方向是**主动**的 —— 先用传播「制造」确定边界，再分解。即「deduction engine 的副产品 = 分解切分」（见 [21](21-deduction-engine.md)）。
- 对 13×13+ 大网格（档⑤ 根本难，如 0681/1258/0680/1080）尤其关键。

## 3. 现状与代码位置
- 传播线索：`shapes.rs:115` `area_bounds`（含 compass 派生 min）、`backtrack.rs:1243` `check_vertex_ring_ok`（顶点度下界）、`prototypes.rs` 的 BF/SAT/GF(2)。
- DSU 边界：`11` §3.4 指出 DSU 仅对「ring+brick 推导出的 NonBoundary 边」成立，**fence 不成立**（dihedral 类不固定边，已证伪）。
- 无「分解后分别求解再拼回」的调度逻辑（`mod.rs` 只有串行模块链）。

## 4. 收益
- 对抗天花板（档⑤）的少数可用手段之一；大网格收益最大。
- 子问题更小 → 各模块（aog/rose/edge_csp）在子网格上都更快。

## 5. 代价与风险
- **风险：中**。拼回时需保证跨切边界一致（切分处两边必须形成合法区域边界）；传播推断的边界必须 sound（否则剪掉正确解）。
- **代价**：中–大（~400–800 行：边界推断 + 连通分量切分 + 子问题求解 + 拼回校验）。

## 6. 优先级 / ROI
- **P2**，ROI 高（结构性收益，但依赖 [21](21-deduction-engine.md) 先提供足够推断出的边界；24 N5）。

## 7. 实现思路
```
// mod.rs 预搜索阶段，DFS 之前
// 1. 跑 deduction engine（见 21）→ 得到 forced Boundary / NonBoundary 边集
// 2. 用 forced NonBoundary 边做 DSU 合并（仅 sound 的来源，不含 fence 推导）
// 3. 用 forced Boundary 边切网格 → 连通分量 {S1..Sk}
// 4. 若 k > 1：对每个 Si 独立调用 solver::solve(sub_puzzle_i)
// 5. 拼回：合并各子解的区域（region id 偏移），跨切边界天然是 Boundary
// 6. 出口：validate 全量一次（兜底）
```
- 切分前提校验：每个 Si 内部必须自身可解（面积/规则自洽），否则回退整盘求解。

## 8. 验证方法
- 单元：构造已知可切分题，断言子解拼回 == 整盘解。
- `--baseline` REGRESSION=0；重点看大网格题 NEW 解出与 wall 下降。
- soundness：断言「被推断为 Boundary 的边」在官方解中确实是边界（用 `-answer` 语料校验）。

## 9. 依赖与前置
- 前置（强）：[21-deduction-engine.md](21-deduction-engine.md)（提供 forced 边集）。
- 协同：[19-simd-flood.md](19-simd-flood.md)（连通分量切分更快）。

## 10. 参考
- `docs/优化/24` §11.5；`11` §3.6（降维分解 ✅ 高收益）、§3.4（DSU 边界）；`21`（档⑤ 大网格）。
