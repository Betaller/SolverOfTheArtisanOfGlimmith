# 34 · 搜索过程追踪与分析工具

> 状态：🟢 新方向 ｜ 分类：工具 / 可观测性 ｜ 来源：本文新调研（24 未覆盖）
> 关联：[25-diff-fuzz.md](25-diff-fuzz.md) · [14-meta-determinism.md](14-meta-determinism.md) · [15-meta-evalproto.md](15-meta-evalproto.md)

## 1. 一句话
构建「搜索过程可观测」的工具链：记录分支树、每个分支的耗时/节点数/剪枝命中、失败题的卡点，让「优化哪里」由**数据**决定而非直觉 —— 是所有后续优化方向的**元方向**。

## 2. 思想（为什么有效）
- 当前优化方向的选择主要靠**代码阅读 + 直觉**（过去 23 篇调研也如此）。但搜索行为有强实例依赖性：某个剪枝在 A 题救命、在 B 题纯属浪费。
- 有了追踪工具就能回答关键问题：
  - 这道 FAIL 题，时间花在哪个模块、哪一层、哪个分支？
  - 哪个剪枝命中率最高/最低（低命中 = 纯开销，可删除提速）？
  - 搜索树是否极度不平衡（说明变量序有问题）？
  - 回溯热点集中在哪些格/边（说明那里缺传播）？
- 这类「profiling + 可视化」在现代求解器工程中是标配（SAT 求解器的 `--trace`、CP 的搜索树可视化），本项目缺失。

## 3. 现状与代码位置
- 已有粗粒度追踪：`attempts`（per-module status/elapsed，`23`）—— 但**无搜索树内部**信息。
- 无分支级埋点、无剪枝命中计数、无搜索树导出。
- `Solution.steps_taken` 恒 0（`TODO.md` P2 #6 标注废弃）—— 说明曾有步数统计的意图但未落地。

## 4. 收益
- **让所有其他方向的优先级有据可依**：例如若发现 `empty_area_check` 占 60% 时间，则 [19-simd-flood.md](19-simd-flood.md) 立即升为 P0；若发现某剪枝命中率 <1%，可直接删除提速。
- 加速每一轮优化的迭代（先测再改，而非先改再测）。
- 副产品：为 [05-ml-routing.md](05-ml-routing.md)/[06](06-ml-ordering.md) 提供训练轨迹数据。

## 5. 代价与风险
- **风险：低**（纯埋点 + 离线分析，默认关闭不影响生产路径）。
- **代价**：小–中（~200–400 行：埋点 + JSONL 导出 + 分析脚本）。注意埋点在热路径的开销 → 用编译 feature 或 env 开关，默认零开销。

## 6. 优先级 / ROI
- **P1**，ROI 高（元方向：本身不提速，但让后续每个方向都更准；且成本小）。

## 7. 实现思路
```
// 1. 埋点（feature gate，默认 off）
#[cfg(feature = "trace")]
struct Trace { branch_events: Vec<BranchEvent>, prune_hits: [u32; N_PRUNES], node_time: ... }
// BranchEvent { depth, cell, chosen_rid, subtree_nodes, backtracked: bool, t_us }
// 2. 导出：RSOLVER_TRACE=1 → 写 results/tmp/trace-<puzzle>.jsonl
// 3. 分析脚本 scripts/analyze_trace.py：
//    - 剪枝命中率排行（找出"零命中"剪枝 = 纯浪费）
//    - 耗时热点（按剪枝/按分支聚合）
//    - 搜索树不平衡度（子树大小分布）
//    - 回溯热点格（哪些格反复被推翻 → 那里缺传播）
// 4. 可选可视化：搜索树 D3/graphviz 导出
```
- 与 [25-diff-fuzz.md](25-diff-fuzz.md) 共用剪枝开关基建。

## 8. 验证方法
- 埋点开关关闭时：`--baseline` 结果与耗时与基线一致（证明零开销）。
- 分析脚本在若干 FAIL 题上产出可读报告（人工验证结论合理）。

## 9. 依赖与前置
- 无强前置；与 [25](25-diff-fuzz.md)（剪枝开关）、[14](14-meta-determinism.md)（可复现）共享基建。
- 输出供 [05](05-ml-routing.md)、[06](06-ml-ordering.md)、[19](19-simd-flood.md) 等使用。

## 10. 参考
- `23`（attempts 追踪已落地，可扩展）；`TODO.md` P2 #6（steps_taken 废弃）；SAT/CP 求解器的 trace 实践。
