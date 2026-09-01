# 14 · 确定性 / 可复现

> 状态：🟢 新方向 ｜ 分类：实验方法论 ｜ 来源：`docs/优化/24` §7.2
> 关联：[08-ml-restart.md](08-ml-restart.md) · [01-parallel-inter.md](01-parallel-inter.md) · [02-parallel-intra.md](02-parallel-intra.md)

## 1. 一句话
搜索依赖 deadline 墙钟 → 同题同机两次可能走到不同深度（非确定性）。引入固定 seed + 把 `RNG_SEED` 记入 `attempts`，让「优化前后」的对比可复现，避免把噪声当改进。

## 2. 思想（为什么有效）
- 当前非确定性来源：deadline 是**墙钟**（`clock.rs`），受 CPU 争抢影响；`-j 8` 并行时同题两次可能在不同深度被打断 → 走不同搜索路径 → 耗时/甚至结果不同。
- 这会污染所有优化评估：「优化后快了 20%」可能只是这次 CPU 更闲。必须：① 用 `attempts` 的 per-module **status**（是否解出）而非总耗时做改进判据；② 需要可复现时注入固定 seed 并记录。
- 并行（[01](01-parallel-inter.md)/[02](02-parallel-intra.md)）与随机化重启（[08](08-ml-restart.md)）会**进一步**降低确定性，故本方向是它们的**前置基建**。

## 3. 现状与代码位置
- deadline：`rsolver/src/clock.rs` + 各模块检查点（`aog/search.rs:858`、`backtrack.rs:470`、`edge_csp/mod.rs:458`、`dlx.rs:192`）。
- 无 RNG seed 概念（搜索完全确定性但受墙钟中断点影响）。
- `attempts` 字段已可记录额外元信息（`23`）。

## 4. 收益
- 优化评估可信：区分「真实改进」与「噪声/调度抖动」。
- 为并发/重启/ML 等引入随机性的方向提供可复现基线。
- 复现 bug（如挂死）时可重放同一次搜索。

## 5. 代价与风险
- **风险：低**（纯基建）。固定 seed 会让「确定性模式」牺牲一点重启带来的随机增益 → 生产可关、调试可开。
- **代价**：小（注入 seed + 写入 `attempts` + bench 支持 `--seed`）。

## 6. 优先级 / ROI
- **P1**，ROI 中（是 [01]/[02]/[08] 的前置；24 §7.2）。

## 7. 实现思路
```
// 1. 注入：新增 RUST_SEED env → 全局 SmallRng(seed)
// 2. 分支点（pick_next_cell / aog 优先级 / 值序）用 rng 做 tiebreak
// 3. 记录：Solution.attempts[i].note 追加 "seed=<n>"
// 4. bench：--seed N（默认随机），复现时显式指定
// 5. 判据：对比优化前后用 per-module status 集合，而非 elapsed_ms
```

## 8. 验证方法
- 同 seed 跑两次：结果 + per-module status 完全一致（耗时允许噪声）。
- 无 seed 时结果仍一致（解唯一），但路径可能不同。

## 9. 依赖与前置
- 是 [08-ml-restart.md](08-ml-restart.md)、[01](01-parallel-inter.md)、[02](02-parallel-intra.md) 的前置。

## 10. 参考
- `docs/优化/24` §7.2；`23`（attempts 字段）；`17`（deadline 盲区）。
