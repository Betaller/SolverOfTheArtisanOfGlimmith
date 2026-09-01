# 36 · Anytime 渐进解 / 部分解返回

> 状态：🟢 新方向 ｜ 分类：交互 / 架构 ｜ 来源：本文新调研（24 未覆盖）
> 关联：[16-incr-solve.md](16-incr-solve.md) · [01-parallel-inter.md](01-parallel-inter.md) · [23-walksat.md](23-walksat.md)

## 1. 一句话
让求解器在搜索过程中**持续输出「当前最佳部分解」**（已确定的区域/边），而不是只在最后返回完整解 —— 超时时用户仍能看到进展，UI 可实时渲染，且部分解可作为下一轮搜索的起点。

## 2. 思想（为什么有效）
- 当前是「全或无」：要么在 timeout 内解出，要么返回 `None`。对超时题（192 FAIL 中的一部分）**零信息产出**，用户体验与可调试性都差。
- 搜索过程中其实已积累了大量**确定性信息**：已固定的区域、已推断的边界、deduction 的结果。这些在超时时本可直接输出。
- anytime 算法的核心性质：**随时可中断并返回当前最优**；中断越晚质量越好。配合 [16-incr-solve.md](16-incr-solve.md) 的增量求解，部分解可作为下一轮的 warm-start（类 [22-relax-warmstart.md](22-relax-warmstart.md)）。
- 对 UI：可渲染「已确定区域」+「未定区域灰色」，用户看到求解器在推进，而不是白屏等 15s。

## 3. 现状与代码位置
- 返回结构：`Solution { solved, regions, elapsed_ms, rule_results, solver, attempts, ... }`（`types.rs:231`）—— 只有 `solved` 布尔，无「部分解」概念。
- 搜索中间状态：`BacktrackState`（`backtrack.rs:108`）的 `cell_to_region`、`edge_csp` 的 `edges` —— 都是可得的部分赋值，但未导出。
- `ModuleOutcome`（`types.rs:171`）有 `ValidationFailed` / `None`，无 `Partial`。
- UI：`src/ui/solver_runner.py` 阻塞等待最终结果。

## 4. 收益
- UX：超时题不再「白等」，可展示进展；交互式求解体验质变。
- 调试：能看到搜索卡在哪片区域（配合 [34-trace-tooling.md](34-trace-tooling.md)）。
- 算法：部分解可作 warm-start / 并发交接（模块 A 超时后把部分解交给模块 B）。

## 5. 代价与风险
- **风险：低**（新增输出类型，不改变完整解的正确性判定；部分解需明确标注 `partial=true` 且不通过 `validate`）。
- **代价**：小–中（~150–300 行：`PartialSolution` 结构 + 导出 + UI 渲染 + 协议扩展）。

## 6. 优先级 / ROI
- **P2**，ROI 中（体验与可调试性收益；与 [16](16-incr-solve.md) 强协同，两者共享状态导出机制）。

## 7. 实现思路
```
// 1. 新增类型
struct PartialSolution { solved: bool, fixed_regions: Vec<RegionInfo>,
                         fixed_edges: Vec<(EdgeId, EdgeState)>, progress: f32 }
// 2. 导出：搜索中周期性（如每 N 节点或每 200ms）把当前固定赋值快照导出
//    - 命令行：RSOLVER_PARTIAL=1 → 每行输出一条 partial（或写侧信道文件）
//    - 库调用：回调 on_progress(PartialSolution)
// 3. UI：solver_runner 接收 partial → grid_widget 渲染（已定区域着色，未定灰）
// 4. 交接：模块超时 → 把 partial 作为下一模块 warm-start（需模块支持部分赋值初始化，见 22）
```
- `progress` 度量：已定格数 / 总格数，或「已定区域数 / K」。

## 8. 验证方法
- 正确性：完整解仍 `solved=true`；部分解标记 `partial=true` 且**不**通过 validate（避免被误当解）。
- UI：手动验证超时题能渲染进展。
- `--baseline` REGRESSION=0（完整解路径不受影响）。

## 9. 依赖与前置
- 状态导出机制与 [16-incr-solve.md](16-incr-solve.md) 共享（可合并实现）。
- 消费方：[22-relax-warmstart.md](22-relax-warmstart.md)（部分解作 warm-start）、[34](34-trace-tooling.md)（调试）。

## 10. 参考
- `types.rs:171,231`；`16-incr-solve.md`；anytime algorithm 概念。
