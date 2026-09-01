# 09 · 消除 backtrack 热路径 HashSet 分配

> 状态：🟢 新方向 ｜ 分类：底层系统调优 ｜ 来源：`docs/优化/24` §6.1
> 关联：[10-low-bitvector.md](10-low-bitvector.md) · [11-low-doublevalidate.md](11-low-doublevalidate.md)

## 1. 一句话
`backtrack.rs` 热路径每树层为每未决格新建一个 `HashSet`（`cell_domain_size`），且 `dfs` 每节点建 `HashSet`/`Vec`——改成「固定小数组 + 计数」或 `u64` 位图，把每节点分配成本降一个数量级。

## 2. 思想（为什么有效）
- `cell_domain_size`（`backtrack.rs:912`）每次调用 `HashSet::new()` 算「相邻区域数」；`pick_next_cell`（`backtrack.rs:1012`）对**每个**未决格调用它 → 16×16 盘每树层 ~256 次 HashSet 分配 + drop。分配器再快也是纯浪费。
- `dfs`（`backtrack.rs:519-549`）每节点建 `rid_set: HashSet<usize>` + `valid_rids: Vec<usize>`。域 = 相邻区域 id 集合，规模 ≤4（上下左右），完全可用栈上小数组 + 计数。
- 域也可用 `u64` 位图（region id < 64），`insert`/`contains`/`count` 是位运算，零分配（见 [10](10-low-bitvector.md)）。

## 3. 现状与代码位置
- `backtrack.rs:912` `cell_domain_size`（每调新建 HashSet）。
- `backtrack.rs:955` `pick_next_cell`（遍历所有未决格调用上述）。
- `backtrack.rs:519-549` `dfs` 节点内 `rid_set`/`valid_rids` 分配。
- 注：`15/17` 已把 `HashMap` state 改 `Vec`，但**热路径 HashSet 分配未清**。

## 4. 收益
- backtrack 单节点成本降一个数量级；虽 backtrack 当前在官方语料几乎不触发（aog 先赢），但它是 [01-parallel-inter.md](01-parallel-inter.md) 并发后可能的 winner，且未来接手大网格直接受益。
- 顺带降 alloc 压力 → 减 GC/碎片（配合 [12-low-allocator.md](12-low-allocator.md)）。

## 5. 代价与风险
- **风险：低**（纯等价重构，不改语义）。需保证域大小 ≤ 数组容量（≤4 邻居 + 新区域）。
- **代价**：小（~100 行，`cell_domain_size` 改计数循环 + 节点结构改栈数组/位图）。

## 6. 优先级 / ROI
- **P1**，ROI 中（低风险、为并发后回退路径铺路；24 §8 速赢 S7）。

## 7. 实现思路
```
// 替代 cell_domain_size 的 HashSet
fn domain_count(cell, cell_to_region, frontier) -> u32 {
    let mut mask: u64 = 0; let mut cnt = 0;
    for n in neighbors(cell) {            // ≤4
        if let Some(r) = cell_to_region[n] {
            let bit = 1u64 << r;
            if mask & bit == 0 { mask |= bit; cnt += 1; }
        }
    }
    cnt   // +1 表示"开新区域"
}
// dfs 节点：rid_set 用 Vec<usize> 栈上 [0; 8] + len，不用 HashSet
```
- 若 region id 可能 ≥64，回退到固定 `Vec<usize>`（≤5 元素）即可，仍免 HashSet。

## 8. 验证方法
- 单元测试：构造小题，断言改前/后解一致（backtrack 单独跑，临时禁用 aog）。
- 全量 `--baseline` REGRESSION=0。

## 9. 依赖与前置
- 前置：[12-low-allocator.md](12-low-allocator.md) 可选（分配器进一步降压）。

## 10. 参考
- `docs/优化/24` §6.1；`15/17`（HashMap→Vec 已落地）；`backtrack.rs:912,955,519`。
