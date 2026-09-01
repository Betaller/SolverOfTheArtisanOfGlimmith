# 22 · 松弛求解 warm-start

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N7）｜ 来源：`docs/优化/24` §11.7
> 关联：[21-deduction-engine.md](21-deduction-engine.md) · [23-walksat.md](23-walksat.md)

## 1. 一句话
先解一个「删除最难 1–2 条规则」的**松弛版**拿到大量已定区域，再把这些区域作为**固定初始赋值**喂给完整搜索（warm-start）——把「全规则同时搜索」拆成「易规则定骨架 + 难规则微调」。

## 2. 思想（为什么有效）
- 失败主因常是**某一条规则缺传播导致爆炸**（`21` §1.2：绝大多数 FAIL 是 2–4 规则组合，瓶颈往往是其中一条缺传播/范式错配）。例如 compass+watchtower：compass 的定位约束强、watchtower 几乎零传播。
- 松弛版（去掉 watchtower）通常能快速解出，且解出的区域划分**大部分是正确的**（watchtower 只是局部微调约束）。把这个解当 warm-start，完整搜索只需在少量区域上调整，搜索空间骤减。
- 类比 SAT 的「core-first / assumption 求解」：先解核心约束，再逐步加回被移除的约束。

## 3. 现状与代码位置
- 规则集可变：`Puzzle.rules: Vec<Rule>`（`rsolver/src/types.rs:123`），构造松弛 puzzle 只需 clone + 移除规则。
- 各模块按规则决定能力：`mod.rs:373-381`（`is_rose_capable`/`is_edge_csp_capable`）。
- 搜索状态初始化：`backtrack.rs:108` `BacktrackState`、`aog/core.rs` —— 当前都从空盘开始，无「预置部分赋值」入口。

## 4. 收益
- 对「某规则缺传播」的题（档②传播缺口 ~25 道、档③范式错配 rose 55 + fence）是针对性解法。
- 与 [21-deduction-engine.md](21-deduction-engine.md) 互补：deduction 是「推」，warm-start 是「解松弛版再修正」。

## 5. 代价与风险
- **风险：中**。松弛解可能**不满足**被移除的规则（需回退/微调）；若松弛解与真解结构差异大，warm-start 反而误导搜索（需加「验证失败则丢弃」的兜底）。
- **代价**：中（~200–400 行：松弛 puzzle 构造 + 部分赋值注入 + 失败回退）。

## 6. 优先级 / ROI
- **P2**，ROI 中（针对性强；24 N7）。

## 7. 实现思路
```
// 1. 识别"最难规则"：按 21 的档位/缺传播情况，或简单按规则类型启发
//    （如 watchtower / compass / fence 常是缺传播者）
// 2. 构造松弛 puzzle：rules.retain(|r| r.ty != HARDEST)
// 3. solve(relaxed) → 得到区域划分 R
// 4. 用 R 作为 warm-start：初始化 cell_to_region = R 的分配
// 5. 在完整 puzzle 上跑搜索，但把 R 作为"优先分支序"（先尝试保持 R）
// 6. 若搜索超时/失败 → 丢弃 warm-start，退回常规求解（保证不劣化）
```
- 安全：warm-start 只影响**分支序/初始值**，不影响正确性（最终仍 validate）。

## 8. 验证方法
- `--baseline` REGRESSION=0（关键：warm-start 不得引入错误解）。
- 关注档②/档③ 题 NEW 解出。
- 统计「松弛解与真解的区域重合度」，评估 warm-start 质量。

## 9. 依赖与前置
- 前置：搜索器需支持「预置部分赋值 / 优先分支序」入口（小改）。
- 协同：[23-walksat.md](23-walksat.md)（另一条求初始解的路径）。

## 10. 参考
- `docs/优化/24` §11.7；`21` §1.2（多规则组合瓶颈）、档②/③。
