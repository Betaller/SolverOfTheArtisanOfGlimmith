# 16 · 增量 / 交互式求解（UI 实时重解）

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N9）｜ 来源：`docs/优化/24` §11.1
> 关联：[18-incr-validate.md](18-incr-validate.md) · [21-deduction-engine.md](21-deduction-engine.md)

## 1. 一句话
当前每次求解都是「整盘从头 DFS」；UI 里用户边画边界、边加线索，每次改动只影响局部。维护「已固定部分赋值 + 约束图」，仅对变更区域做局部重传播 / 局部重搜索，复用未受影响子树的结论。

## 2. 思想（为什么有效）
- 谜题求解是**单调**的：新增 `is_boundary` / 新增线索只会**收紧**约束集，不会放宽。因此上一次搜索中「已被证明无解」的子树仍无解，「已固定的赋值」仍有效——只需重算受影响的局部。
- 这把交互延迟从 O(全搜索) 降到 O(局部)，对大网格（13×13+）手绘体验是质变。
- 与 [21-deduction-engine.md](21-deduction-engine.md) 共享「传播到不动点」的内核；与 [18-incr-validate.md](18-incr-validate.md) 共享「只验变更区域」的思路。

## 3. 现状与代码位置
- 批处理接口：`rsolver/src/main.rs`（stdin JSON → stdout JSON，无状态）。
- Rust 内部状态：`backtrack.rs:108` `BacktrackState`、`edge_csp/mod.rs:42` `Solver`（含 undo trail）。
- UI：`src/ui/grid_widget.py`、`solver_runner.py`；`properties_panel.board_modified` → `grid_widget.update()`（当前只刷新显示，不重解）。
- 约束：`src/solver/rust_solver.py` 每次整盘序列化。

## 4. 收益
- UI 实时求解延迟从「整盘秒级」降到「局部毫秒级」；大网格手绘可用性。
- 也为「逐步揭示线索」的教学/展示场景提供能力。

## 5. 代价与风险
- **风险：中**。增量正确性需保证（作废子树必须准确识别）；可退化为「全解」作兜底（任何不确定时重解全盘）。
- **代价**：中（~300–500 行：状态外化 + 影响域计算 + 增量重搜接口）。

## 6. 优先级 / ROI
- **P2**，ROI 中（体验向，非解出率；24 N9）。

## 7. 实现思路
```
// rsolver 新增接口
solve_incremental(prev_state_json, delta_json) -> {solution, new_state_json}
// prev_state: 上次 cell_to_region + 形状库 + 传播不动点 + 已证无解子树摘要
// delta: 新增/移除的 is_boundary / 线索 / blocked
// 流程：
//   1. 应用 delta 到 prev_state
//   2. 从 delta 影响格出发重跑传播（见 21）
//   3. 若传播后矛盾 → 立即返回 unsat（局部即知，不必全搜）
//   4. 否则只重搜「受影响的连通子区域」，其余沿用旧解
```
- UI：常驻子进程（[03](03-parallel-puzzle.md)）持有 state，避免反复传输整个状态。

## 8. 验证方法
- 等价性：对同一最终谜题，增量求解结果与全量求解一致（官方解比对 `matches_official`）。
- 单调性：仅加约束时，增量结果 ⊇ 旧固定赋值。

## 9. 依赖与前置
- 前置：[18-incr-validate.md](18-incr-validate.md)、[21-deduction-engine.md](21-deduction-engine.md)、[03](03-parallel-puzzle.md)（常驻进程）。

## 10. 参考
- `docs/优化/24` §11.1；`src/ui/grid_widget.py`；`backtrack.rs:108`。
