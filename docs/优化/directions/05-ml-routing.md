# 05 · 学模块路由策略

> 状态：🟢 新方向 ｜ 分类：机器学习 ｜ 来源：`docs/优化/24` §5.1
> 关联：[01-parallel-inter.md](01-parallel-inter.md) · [29-adapt-budget.md](29-adapt-budget.md) · [15-meta-evalproto.md](15-meta-evalproto.md)

## 1. 一句话
2488 道官方题的 `attempts` 追踪（`23` 已落地）构成**免费监督数据集**：用题面特征训练轻量模型预测「哪条模块链最可能先赢」，替代 `mod.rs` 手写的 `is_rose_capable`/`is_edge_csp_capable` 硬规则，给模块排序甚至裁剪。

## 2. 思想（为什么有效）
- 当前路由是**手写 if 规则**（`mod.rs:373-381`）：rose-capable = 有 rose_window 且无 same/different；edge_csp-capable = ring/brick/watchtower/compass/inequality/difference 密集。这些规则是「启发式」，未必最优——有些题虽 rose-capable 但 edge_csp 更快。
- 监督信号已存在：每题的 `attempts` 字段记录了「各模块 status + elapsed」（`results/bench/*.jsonl`）。这是现成的 `(特征 → 最优模块)` 标签。
- 模型可学**非线性交互**（如「compass + 大网格 + 少边界」→ edge_csp 优先），超过手写规则的表达力。

## 3. 现状与代码位置
- 数据：`results/bench/*.jsonl` 的 `attempts`；特征源 `puzzle.rules` / 尺寸 / 线索密度 / pre-boundary 数。
- 路由：`rsolver/src/solver/mod.rs:373-381` capability 判断。
- 追踪：`docs/优化/23`（per-module attempts）。

## 4. 收益
- 减少「串行跑满 5 模块」的无效墙钟；配合 [01-parallel-inter.md](01-parallel-inter.md) 并发，模型给模块排优先级（先跑高置信模块），墙钟再降。
- **风险极低**（纯调度层，不影响搜索正确性、零回归风险）。

## 5. 代价与风险
- **风险：极低**。模型只决定「先跑谁 / 是否跳过」，最终解仍过 `validate` 门。
- **代价**：小–中（特征工程 + 训练一个决策树/小 MLP + 把模型嵌入 `mod.rs`，~150–300 行 + 一个离线训练脚本）。

## 6. 优先级 / ROI
- **P1**，ROI 高（零算法风险、直接降墙钟、数据现成）。24 §8 速赢 S5。

## 7. 实现思路
1. **抽特征**：`puzzle` → 向量（height,width, rule one-hot, #clue cells, #pre_boundaries, area/compass 强度…）。
2. **造标签**：从 jsonl 取每题「首个 `success` 模块」为 label；失败题 label = `none`（路由不改变结局）。
3. **训练**：`sklearn` 决策树 / 梯度提升（可解释、零依赖）；或 tiny MLP。
4. **嵌入**：离线训好 → 导出为 Rust 可读的查表/小决策树（`mod.rs` 内联），避免运行时依赖 Python。
5. **使用**：`solve` 开头用模型给 5 模块排 `priority`，并发时先 dispatch 高 priority。

## 8. 验证方法
- 离线：留出集准确率 / 各模块耗时节省。
- 在线：`--baseline` 对比，要求 REGRESSION=0、NEW=0、墙钟均值下降。
- 模型误判「跳过某模块」不会漏解（并发模式仍跑全部；仅排序场景需保证「被跳过模块在并发下仍参与」或保留 fallback）。

## 9. 依赖与前置
- 依赖 [15-meta-evalproto.md](15-meta-evalproto.md)（用 per-module 耗时而非总墙钟评估）。
- 数据来自 [23](https://) 的 attempts 追踪（已落地）。

## 10. 参考
- `docs/优化/24` §5.1；`23`；`11` §4.3（参考求解器技术吸收）。
