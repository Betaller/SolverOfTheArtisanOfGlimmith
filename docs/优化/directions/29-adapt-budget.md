# 29 · 自适应预算分配

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N13）｜ 来源：`docs/优化/24` §11.14
> 关联：[05-ml-routing.md](05-ml-routing.md) · [01-parallel-inter.md](01-parallel-inter.md) · [13-meta-rustparts.md](13-meta-rustparts.md)

## 1. 一句话
并发均分预算仍浪费。用模型/启发给「最可能解出」的模块**动态多给预算**、弱模块少给 —— 比「并发均分」更聪明，且直接降墙钟。

## 2. 思想（为什么有效）
- [01-parallel-inter.md](01-parallel-inter.md) 让 5 个模块并发抢同一时钟，但**谁都不知自己该拿多少**：一个明显没戏的模块（比如 aog 在 rose 题上，当前已有 `AOG_ROSE_BUDGET_MS=3000` 的硬编码削减，`mod.rs:381`）仍在跑，占用 CPU（并发下还抢缓存），拖慢真正能解出的模块。
- 现状已有**一处粗粒度自适应**：rose-capable 题给 aog 只 3s（`AOG_ROSE_BUDGET_MS`，`mod.rs:381`）。本方向是把它**推广为通用机制**：用题面特征 + 早期信号动态分配。
- 两种信号：
  - **先验**（题面特征）：用 [05-ml-routing.md](05-ml-routing.md) 的模型预测各模块胜率 → 按胜率分配预算。
  - **在线**（早期信号）：模块跑了一小段时间后的进展（搜索节点数增长率、deadline 前剩余不确定性）→ 动态加/减预算（类似 OS 调度）。

## 3. 现状与代码位置
- 硬编码特例：`mod.rs:381` `AOG_ROSE_BUDGET_MS = 3000`（rose-capable 题给 aog 3s）。
- 通用分配：无 —— 每模块收完整 `timeout_ms`（`mod.rs:53-56`）。
- deadline 检查点：`aog/search.rs:858`、`backtrack.rs:470`、`edge_csp/mod.rs:458`、`dlx.rs:192`（可注入 per-module deadline）。

## 4. 收益
- 在 [01](01-parallel-inter.md) 并发基础上再砍无效墙钟（弱模块早停 → 强模块拿到更多 CPU/缓存）。
- 单线程模式下同样有效（弱模块早停，把预算让给下一个模块 —— 直接减少串行浪费）。

## 5. 代价与风险
- **风险：低**（纯调度，不改搜索语义；正确性由后续模块 + 独立验证器兜底）。
- **代价**：小（~100–250 行：per-module deadline 注入 + 分配策略 + 早期信号采集）。
- **注意**：分配过于激进可能饿死「后发制人」的模块（实测 21 道由 edge_csp 在 aog 超时后解出、9 道由 rose 解出 —— 若 aog 拿走全部预算这些就没了）。需保留最小保障预算。

## 6. 优先级 / ROI
- **P1**，ROI 高（零算法风险、与 [01](01-parallel-inter.md)/[05](05-ml-routing.md) 天然协同；24 N13）。

## 7. 实现思路
```
// 1. per-module deadline：solve_with_deadline(puzzle, deadline_ms) 取代统一 timeout_ms
// 2. 先验分配：用 [05] 的模型输出胜率 p_i，按 p_i 分配基础预算
//    budget_i = max(MIN_GUARANTEE, timeout * p_i)      // MIN_GUARANTEE 防饿死
// 3. 在线调整（可选）：
//    - 模块定期汇报进展（已搜节点数 / 剩余不确定格数）
//    - 调度器按"进展速率"重分配剩余预算
// 4. 推广现有特例：把 AOG_ROSE_BUDGET_MS 改成策略的一档（而非硬编码 if）
```
- 最小保障预算：实测中 aog 超时后 edge_csp/rose 解出 30 道（`21+9`），故 aog 不应拿满；建议 aog 上限 ~50–70% timeout，其余留给后继。

## 8. 验证方法
- `--baseline` REGRESSION=0（关键：不能因饿死而丢解）+ NEW 不减。
- 对比「均分并发」vs「自适应分配」的墙钟与解出数。
- 监控：记录各模块实际拿到的预算与是否解出，回流给 [05](05-ml-routing.md) 训练。

## 9. 依赖与前置
- 前置：[01-parallel-inter.md](01-parallel-inter.md) 的共享时钟/取消原语（可复用）。
- 增强：[05-ml-routing.md](05-ml-routing.md) 提供胜率先验。
- 评估：[13-meta-rustparts.md](13-meta-rustparts.md)、[15-meta-evalproto.md](15-meta-evalproto.md)。

## 10. 参考
- `docs/优化/24` §11.14；`mod.rs:381`（AOG_ROSE_BUDGET_MS 现有特例）；实测模块链分布（aog 首解 1034/1066，edge_csp 21、rose 9 在 aog 超时后解出）。
