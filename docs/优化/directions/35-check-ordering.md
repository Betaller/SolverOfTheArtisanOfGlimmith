# 35 · 约束检查排序 / fail-fast

> 状态：🟢 新方向 ｜ 分类：搜索 / 常数因子 ｜ 来源：本文新调研（24 未覆盖）
> 关联：[34-trace-tooling.md](34-trace-tooling.md) · [18-incr-validate.md](18-incr-validate.md) · [09-low-hashset.md](09-low-hashset.md)

## 1. 一句话
把每个分支点的守卫链（多个 `check_*`）按**「便宜的先跑、命中率高的先跑」**排序，让矛盾尽可能早、尽可能廉价地被发现 —— 纯常数因子优化，零语义改动。

## 2. 思想（为什么有效）
- 分支点当前是一串固定顺序的守卫：`check_watchtowers_ok && check_vertex_ring_ok && check_sealed_regions && check_shape_pattern_ok && check_fence_ok`（`backtrack.rs:611-616,651-656`）。顺序是**历史形成的**，不是按成本/收益排的。
- 若把「O(1) 且常失败」的检查放在最前，绝大多数非法分支在几条指令内就被否掉，昂贵的检查（如全盘扫描、形状匹配）根本不用跑。
- 期望成本公式：把检查 i 排在 j 前，当且仅当 `cost_i + p_fail_i_shorter...` —— 经典做法是按 `cost_i / P(i 失败)` 升序（单位否定成本最小者先跑）。
- 这是**编译期/离线可算**的排序：用 [34-trace-tooling.md](34-trace-tooling.md) 的命中率数据算出最优顺序，甚至可以每题动态排序。

## 3. 现状与代码位置
- backtrack 守卫链：`backtrack.rs:611-616,651-656`。
- aog 守卫：`aog/search.rs` 各 `check_*`（`check_edge`/`check_tatami`/`check_loopy`/`check_radar`/…）。
- `validate.rs` 的 22 规则也是固定顺序扫描。
- 成本差异确实存在：`check_sealed_regions`（`backtrack.rs:750`，区域级）vs `check_vertex_ring_ok`（`:1243`，顶点级）vs `check_shape_pattern_ok`（`:679`，查预计算表）。

## 4. 收益
- 分支点常数因子下降（可能 20–50%，取决于命中率分布）→ 惠及所有 DFS 题。
- 零语义风险（检查集合不变，只改顺序）。
- 顺带发现「零命中检查」—— 直接删除（与 [34](34-trace-tooling.md) 联动）。

## 5. 代价与风险
- **风险：极低**（只换顺序，逻辑等价）。唯一注意：某些检查可能有副作用（如顺带更新缓存），需确认是纯函数。
- **代价**：极小（~50–150 行：重排 + 可选的动态排序表）。

## 6. 优先级 / ROI
- **P1**，ROI 高（极低成本、零风险；且需要先有 [34](34-trace-tooling.md) 的数据才能排得准 —— 可先按直觉粗排，再用数据精调）。

## 7. 实现思路
```
// 1. 埋点统计（用 34）：每个 check 的 (平均耗时, 失败率)
// 2. 离线算出最优顺序：按 cost_i 升序 且 优先高失败率者
//    实用近似：按 (cost_i / max(fail_rate_i, ε)) 升序
// 3. 静态重排守卫链
// 守卫链示例（示意，实际顺序由数据决定）：
if !check_cheap_common(..) { return false; }        // O(1)，常失败
if !check_vertex_ring_ok(..) { return false; }      // O(顶点)，中
if !check_sealed_regions(..) { return false; }      // O(区域)
if !check_shape_pattern_ok(..) { return false; }    // 查表，但命中率低则后置
if !check_fence_ok(..) { return false; }            // 贵，最后
// 4. 可选：每题/每规则组合一张顺序表（离线算好，运行时查表）
```
- 对 `validate.rs` 的 22 规则同样处理（出口验证也要快）。

## 8. 验证方法
- 正确性：`--baseline` REGRESSION=0（顺序不影响结果）。
- 收益：节点/秒 吞吐对比（用 [15-meta-evalproto.md](15-meta-evalproto.md) 的 per-module elapsed 判据）。

## 9. 依赖与前置
- 数据前置（强烈建议）：[34-trace-tooling.md](34-trace-tooling.md) 提供命中率。
- 协同：[18-incr-validate.md](18-incr-validate.md)（增量验证的检查排序同样适用）。

## 10. 参考
- `backtrack.rs:611-616,651-656`；`34-trace-tooling.md`；经典「fail-fast / cheapest-first」守卫排序。
