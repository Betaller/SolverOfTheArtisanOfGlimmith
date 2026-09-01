# 08 · 随机化重启

> 状态：🟢 新方向 ｜ 分类：机器学习（探索杠杆）｜ 来源：`docs/优化/24` §5.4
> 关联：[07-ml-nogood.md](07-ml-nogood.md) · [06-ml-ordering.md](06-ml-ordering.md) · [01-parallel-inter.md](01-parallel-inter.md)

## 1. 一句话
对单模块搜索，周期性随机重置根（保留学到的 nogood / 形状跳过索引），换一个变量序再搜——cube-and-conquer 风格。成本极低，对「单一模块差一点」的超时题常能撞到更短路。

## 2. 思想（为什么有效）
- 手写 DFS 是**确定性**的：同一题同机两次走同一路径。若这条路恰好很长（撞搜索空间坏区），它会一直慢。随机化重启引入「多样性」：每次重启用不同 seed 的变量序，可能绕开坏区。
- 这是「无模型」探索杠杆，不需训练数据，比 [06]/[07] 的 ML 更轻；且能与它们叠加（重启间保留 nogood）。

## 3. 现状与代码位置
- 当前搜索完全确定性（无 RNG 注入分支点）。`deadline` 检查点存在但无重启逻辑。
- 类比：SAT 求解器的「restart + 保留 learned clause」。

## 4. 收益
- 对超时档（~45 道）中「解存在但路径长」的题，重启可能直接命中。
- 零算法改动、零数据需求，纯搜索策略增强。

## 5. 代价与风险
- **风险：低**。重启不改变正确性；但确定性丢失（回归比对需 `--retry-timeouts` / 固定 seed，见 [14-meta-determinism.md](14-meta-determinism.md)）。
- **代价**：小（加 RNG seed + 重启计数器 + deadline 切片，~80–150 行）。

## 6. 优先级 / ROI
- **P2**，ROI 中（廉价探索杠杆，与 [01](01-parallel-inter.md) 并发天然兼容：并发跑「不同 seed 的多副本」也是 restart 的一种）。

## 7. 实现思路
```
let mut seed = rng_seed();
loop {
    let sol = module.solve_with_seed(puzzle, timeout_slice, seed);
    if let Solved(s) = sol { return s; }
    if wall_elapsed() > timeout_ms { break None; }
    seed = next_seed(seed);          // 换变量序/值序
    // 保留 nogood 缓存（见 07）跨重启累积
}
```
- 与并发结合：直接起 K 个「不同 seed」副本并发（[01](01-parallel-inter.md) 的 first-wins），等价于 restart 的并行版，更稳。

## 8. 验证方法
- `--baseline` 对比：REGRESSION=0、关注超时档 NEW 解出。
- 固定 seed 应可复现（[14](14-meta-determinism.md)）。

## 9. 依赖与前置
- 依赖 [14-meta-determinism.md](14-meta-determinism.md)（记录 seed）。
- 可与 [01](01-parallel-inter.md) 合并实现（多 seed 并发）。

## 10. 参考
- `docs/优化/24` §5.4；SAT restart / cube-and-conquer 文献。
