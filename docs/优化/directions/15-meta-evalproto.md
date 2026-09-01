# 15 · 评估协议

> 状态：🟢 新方向 ｜ 分类：实验方法论 ｜ 来源：`docs/优化/24` §7.3
> 关联：[13-meta-rustparts.md](13-meta-rustparts.md) · [14-meta-determinism.md](14-meta-determinism.md) · [05-ml-routing.md](05-ml-routing.md)

## 1. 一句话
固化一套「每次优化提交必须附什么」的评估协议：**用 per-module status 而非总墙钟做判据**，并显式隔离「算法改进」与「预算利用率改进」——否则并发/墙钟类改动会被误读成算法收益。

## 2. 思想（为什么有效）
- 现状评估口径混乱：总 `elapsed_ms` 是子进程墙钟，被 `RUST_PARTS=4` 放大（[13](13-meta-rustparts.md)）且含前序模块的浪费；CPU 争抢（`-j 8`）引入噪声（[14](14-meta-determinism.md)）。
- 若不隔离，一次「改了并发」的提交会显示「解出 +5、耗时 -60%」，让人以为是算法突破，实际只是预算利用率提升。**这会让后续所有方向排序失真。**
- 正确判据：`attempts[].status`（solved/timeout/exhausted/validation_failed）是**离散、抗噪**的；`attempts[].elapsed_ms` 是**单模块 CPU 耗时**，不受墙钟放大影响——这才是「算法是否变快」的真实指标。

## 3. 现状与代码位置
- `benchmark_rust_solver.py` 已有 `--baseline` 回归模式（REGRESSION/NEW 检测）、`--out` JSONL。
- `attempts` 追踪已落地（`23`），字段 `solver/status/elapsed_ms/note`。
- 缺：以 `attempts` 为主判据的对比脚本 / 提交模板。

## 4. 收益
- 优化方向排序可信（避免把「基建收益」记在「算法」头上）。
- 新方向（ML/并发/编译）落地时能量化真实增量。

## 5. 代价与风险
- **风险：低**（流程约定 + 一个小脚本）。
- **代价**：小（脚本 ~100 行 + 文档模板）。

## 6. 优先级 / ROI
- **P1**，ROI 高（是所有新方向评估的地基；24 §7.3）。

## 7. 实现思路
```
# scripts/compare_attempts.py（新增）
# 输入：baseline.jsonl, current.jsonl
# 输出：
#   - per-module: 解出数 delta（aog +3, rose -1, edge_csp +12 ...）
#   - per-module: elapsed_ms p50/p90 delta（单模块 CPU 耗时，抗放大）
#   - 分类：REGRESSION / NEW / BUDGET-GAIN（并发类）/ ALGO-GAIN（算法类）
# 判据：
#   算法改进  = 同模块 elapsed_ms 下降 或 同模块解出数上升
#   预算改进  = 总墙钟下降但 per-module elapsed 不变
```
- 提交模板（写进 PR/文档）：
  1. `results/bench/<日期>_<sha>_<msg>.txt`（全量，`AGENTS.md` 要求）
  2. `--baseline` 的 REGRESSION/NEW 对比
  3. **单一模块 CPU 耗时**对比（非放大墙钟）
  4. 若改路由/并发：单独报告「并发前/后」解出数

## 8. 验证方法
- 用已知「纯基建」改动（如改 `RUST_PARTS`）验证脚本能把它归类为 BUDGET-GAIN 而非 ALGO-GAIN。

## 9. 依赖与前置
- 依赖 [13](13-meta-rustparts.md)（去放大后读数才真实）、[14](14-meta-determinism.md)（seed 可复现）。
- 供 [05-ml-routing.md](05-ml-routing.md) 等新方向落地时评估。

## 10. 参考
- `docs/优化/24` §7.3；`23`；`scripts/README.md`。
