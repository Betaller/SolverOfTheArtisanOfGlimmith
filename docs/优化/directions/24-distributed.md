# 24 · 跨机 / GPU 穷举

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N14，长期）｜ 来源：`docs/优化/24` §11.9
> 关联：[02-parallel-intra.md](02-parallel-intra.md) · [04-parallel-gpu.md](04-parallel-gpu.md) · [20-active-decomp.md](20-active-decomp.md)

## 1. 一句话
1434 类「参考求解器也解不动」的题，单核无解。把「形状放置候选 × 子区域划分」分发到多机（或 GPU）做穷举/验证 —— 是**突破天花板**（档⑤）的最后手段。

## 2. 思想（为什么有效）
- 档⑤（~40 道根本难）的特征是「片大 + 每类型格数>2 + 复杂外形」「compass+solitary 大网格」「多规则组合爆炸」——单核 40s 内搜索空间根本走不完（`21` §2 档⑤）。
- 这类题若**可分解**（[20-active-decomp.md](20-active-decomp.md)）或**候选可枚举**（形状放置无依赖，GPU/多机友好），就能把单机不可能的穷举变成分布式可行。
- 与 [04-parallel-gpu.md](04-parallel-gpu.md) 的区别：GPU 只适合规整批处理；**多机 CPU 集群**更适合承载 DFS 子树（保留完整搜索逻辑，只是分片）。

## 3. 现状与代码位置
- 单机单进程：`rsolver/src/main.rs`（batch 模式逐题串行）。
- 并发基建：无（[01](01-parallel-inter.md)/[02](02-parallel-intra.md) 尚未落地）。
- 可枚举阶段：`pieces.rs:207` `generate_all_placements`、rose `region_match.rs:53` 候选 BFS、aog `search.rs:647-708` 元组枚举。

## 4. 收益
- 可能解锁当前 0 解出的 ~40 道根本难题中的一部分（唯一能突破 `21` 天花板 ~1120–1160 的路径之一）。

## 5. 代价与风险
- **风险：高**。分布式带来的确定性丢失、网络/节点故障、结果校验、成本；且这类题可能**本质上**单机也永远解不出（参考求解器同样解不动）。
- **代价**：大（调度器 + 分片协议 + 容错 + 集群资源；数千行 + 运维）。

## 6. 优先级 / ROI
- **⚪ 低优先级（长期）**，ROI 低–不确定。仅在档⑤ 攻坚阶段考虑；24 N14。**先做完 [01]/[02] 单机并发与 [20] 分解再评估。**

## 7. 实现思路
- **分片维度**：① 根级分片（每个 worker 取一个「首区域放置」分支深搜）；② 子区域分片（配合 [20] 分解，各子问题分到不同节点）；③ 候选枚举分片（形状/compass 候选批量验证）。
- **协议**：worker 领任务 → 本地 DFS（带 deadline）→ 回传解/失败；coordinator first-wins 取消。
- **容错**：任务超时重派；worker 崩溃不影响整体。
- **GPU**：仅用于③（候选批量比对，无依赖、规整）。

## 8. 验证方法
- 先在单机的 [02-parallel-intra.md](02-parallel-intra.md) 上验证分片策略有效（多核加速比），再考虑跨机。
- 结果正确性：任何 worker 的解都过 `validate` + `matches_official`。

## 9. 依赖与前置
- 强前置：[01-parallel-inter.md](01-parallel-inter.md)、[02-parallel-intra.md](02-parallel-intra.md)、[20-active-decomp.md](20-active-decomp.md)。
- 延伸：[04-parallel-gpu.md](04-parallel-gpu.md)（候选批量）。

## 10. 参考
- `docs/优化/24` §11.9；`21` §2 档⑤（根本难）、§3（天花板）；`11` §4.5C。
