# 13 · RUST_PARTS 墙钟放大修正

> 状态：🟢 新方向 ｜ 分类：实验方法论 ｜ 来源：`docs/优化/24` §7.1
> 关联：[01-parallel-inter.md](01-parallel-inter.md) · [15-meta-evalproto.md](15-meta-evalproto.md)

## 1. 一句话
`rust_solver.py:147,169` 把子进程墙钟预算设为 `RUST_PARTS(4) × timeout × 1.2`，因 Rust 串行跑 4 模块各拿 timeout。**这把「单题耗时」读数高估 4×**，误导所有优化评估 —— 并发改造后须改为 ×1，否则「超时题」会被虚假放大。

## 2. 思想（为什么有效）
- 串行时最坏墙钟 = 4~5×timeout（每个模块各跑满自己的 `timeout_ms` deadline）。Python 侧为「保证每个模块拿到预算」给子进程 4× 墙钟。
- 副作用：`benchmark` 报告的 `elapsed_ms` 是「子进程墙钟」，含前面模块耗尽的 timeout，**不能反映真正解出的模块的 CPU 时间**。任何「优化 X 后耗时 -30%」的读数都可能只是墙钟构成变化，而非真实加速。
- 并发改造（[01](01-parallel-inter.md)）后，模块抢同一时钟，墙钟 = max(各模块实际耗时) ≈ 最快模块耗时，故 `RUST_PARTS` 应降为 1，耗时读数才真实；且会立刻让很多「超时题」因「墙钟不含前序浪费」而变为「在 timeout 内解出」。

## 3. 现状与代码位置
- `src/solver/rust_solver.py:147` `RUST_PARTS = 4`
- `:169` `_wall_budget` = `timeout * RUST_PARTS * SLACK`
- `:283` 注释说明每题拿 `RUST_PARTS × unit` 墙钟。

## 4. 收益
- 评估正确性：优化对比基于「真实模块 CPU 耗时」而非放大墙钟。
- 并发后立竿见影：虚假超时消失，解出数上升（即使算法未改）。

## 5. 代价与风险
- **风险：极低**（纯 Python 侧常数）。注意：并发改造**前**不能单独改 `RUST_PARTS=1`（会饿死后续模块），须与 [01](01-parallel-inter.md) 同步。
- **代价**：极小（改 1 行 + 注释）。

## 6. 优先级 / ROI
- **P1**，ROI 高（评估纪律，零算法风险；24 §8 隐含在 S1）。

## 7. 实现思路
```
# 并发改造落地时：
RUST_PARTS = 1            # 模块抢同一时钟，墙钟 = max(实际耗时)
# _wall_budget = timeout * 1 * SLACK
# 评估时改用 attempts[].elapsed_ms（每模块独立耗时）做对比，而非总 elapsed
```
- 评估口径：对比优化**前后**用 `attempts` 的 per-module `elapsed_ms`（已有字段，不受墙钟放大影响）。

## 8. 验证方法
- 改前/后同题 `attempts` 应一致；总 `elapsed_ms` 在并发后应下降（因不再含前序浪费）。
- `--baseline` REGRESSION=0。

## 9. 依赖与前置
- 前置：[01-parallel-inter.md](01-parallel-inter.md)（并发改造决定是否改 `RUST_PARTS`）。

## 10. 参考
- `docs/优化/24` §7.1；`rust_solver.py:147,169,283`。
