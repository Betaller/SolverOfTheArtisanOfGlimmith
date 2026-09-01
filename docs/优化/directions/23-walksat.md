# 23 · WalkSAT 快路径

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N8）｜ 来源：`docs/优化/24` §11.8
> 关联：[22-relax-warmstart.md](22-relax-warmstart.md) · [08-ml-restart.md](08-ml-restart.md) · [01-parallel-inter.md](01-parallel-inter.md)

## 1. 一句话
随机初始化区域归属 → 迭代「翻转最小违反数的单元格/边」（min-conflict / WalkSAT 风格）→ 常比 DFS 更快撞到解。作为**快路径先试**，失败再退回 DFS。

## 2. 思想（为什么有效）
- DFS 是**系统搜索**：完备，但在解「稠密」的实例上会先遍历大量坏分支。局部搜索是**非完备但快**：对解空间大、约束松的实例，随机游走 + min-conflict 常在多项式时间撞到解。
- 官方题唯一解，但唯一解不代表解空间稀疏 —— 少线索 / 大网格 / 约束松的题，局部搜索仍可能有效。
- **只作为快路径**：先跑，撞到即赢；跑不动就 DFS。不完备性由 DFS 兜底，故**零正确性风险**。
- 与 [01-parallel-inter.md](01-parallel-inter.md) 天然兼容：作为第 6 个「模块」并发跑，first-wins。

## 3. 现状与代码位置
- 无局部搜索实现（全部是 DFS / DLX / edge-CSP）。
- 违反数计算可复用 `validate.rs` 的规则检查器（逐规则统计违反数，而非只返回 bool）。
- 区域状态表示：`backtrack.rs:108` `cell_to_region: Vec<Option<usize>>`（翻转 = 改一个格的区域归属）。
- deadline 基建已有（`clock.rs`、各模块检查点）。

## 4. 收益
- 对「DFS 分支爆炸但解其实稠密」的题是潜在数量级加速（档⑤ 中的松约束题）。
- 作为并发第六模块，等于给每道题多一次「低成本抽签」机会。

## 5. 代价与风险
- **风险：低**（不完备性由 DFS 兜底；只影响速度不影响正确性）。
- **代价**：小–中（~250–400 行：违反数增量计算 + min-conflict 翻转 + 随机重启 + 与并发框架对接）。

## 6. 优先级 / ROI
- **P2**，ROI 中（廉价快路径，与并发框架复用；24 N8）。

## 7. 实现思路
```
// solver/localsearch.rs（新增）
fn solve_local(puzzle, deadline) -> Option<Solution> {
    let mut assign = random_init(puzzle, &mut rng);   // 每格随机区域 / 随机边界
    for _ in 0..MAX_FLIPS {
        if deadline() { return None; }
        let cost = violation_count(puzzle, &assign);  // 各规则违反数加权和
        if cost == 0 { return Some(assign); }
        // min-conflict：在违反最严重的格/边中选一个翻转，使 cost 下降最大
        let (cell, new_rid) = pick_min_conflict_flip(puzzle, &assign);
        assign.apply(cell, new_rid);
        if stalled() { assign = random_restart(puzzle, &mut rng); }  // 陷入局部最优则重启
    }
    None
}
```
- 违反数需**增量**维护（翻转只影响局部规则），否则每次 O(HW)。
- 权重：不同规则违反的严重度不同，可学习（结合 [28-ml-deep.md](28-ml-deep.md)）。
- 接入：作为 `mod.rs` 并发链的第 6 个参与者（first-wins）。

## 8. 验证方法
- 正确性：局部搜索返回的解仍过 `validate::validate` + Python `IndependentValidator` + `matches_official`。
- `--baseline` REGRESSION=0；统计「局部搜索首解」的题数与 wall 收益。

## 9. 依赖与前置
- 依赖 [01-parallel-inter.md](01-parallel-inter.md)（并发框架，作为第六模块接入）。
- 需要 `validate` 暴露「逐规则违反数」接口（小改）。

## 10. 参考
- `docs/优化/24` §11.8；`08-ml-restart.md`（重启思想）；`01-parallel-inter.md`；WalkSAT / min-conflict 文献。
