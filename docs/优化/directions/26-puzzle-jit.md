# 26 · 编译式特化（puzzle-JIT）

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N11）｜ 来源：`docs/优化/24` §11.11
> 关联：[27-corpus-cache.md](27-corpus-cache.md) · [19-simd-flood.md](19-simd-flood.md) · [12-low-allocator.md](12-low-allocator.md)

## 1. 一句话
把**一道具体 puzzle 的 22 条规则编译成一段特化代码/字节码**（部分求值），去掉所有「if rule_present」分支与数据泛型 —— 类似 SQLite prepared statement。对重复求解同题（全量回归、CI、官方解比对）收益大。

## 2. 思想（为什么有效）
- 当前求解器是**通用解释器**：每个检查点都要判断「规则 X 是否启用」（`puzzle.rules` 里查找）+ 走泛化的数据路径。但一道题的规则集在求解期间**恒定不变**——这些判断是纯浪费。
- 部分求值（specialization）：给定 puzzle，生成一个**只含该题启用规则**的检查序列（去掉未启用规则的分支），并把常量（网格尺寸、区域数上界、area 目标）内联。分支预测 + 内联友好 → 常数因子下降。
- 编译成本一次性摊销：**同题跑多次**（benchmark 全量回归每天跑同一批题；`--retry-timeouts` 重跑；`--baseline` 对比重跑）时总收益为正。

## 3. 现状与代码位置
- 规则集：`Puzzle.rules: Vec<Rule>`（`rsolver/src/types.rs:123`）；检查器分散在 `validate.rs`、`aog/search.rs`（`check_edge`…）、`backtrack.rs`（`check_*`）。
- 无代码生成/特化机制。
- 相关优化已做：aog 用 `OnceLock` 缓存 `aog_debug_enabled()`（`lib.rs:41`）—— 是「特化」的最小雏形。

## 4. 收益
- 热路径分支预测 + 内联 → 常数因子 10–30%（对 aog 主热点尤其）。
- 配合 [27-corpus-cache.md](27-corpus-cache.md)（结果缓存）在重复求解场景收益叠加。

## 5. 代价与风险
- **风险：中**。特化器本身需维护（规则新增/语义变更时要同步）；生成代码的正确性必须严格验证（错误的特化 = 错误剪枝）。
- **代价**：大（~800–1500 行：特化器/字节码 VM 或 codegen + 验证）。
- **适用面窄**：只对「同题重复求解」有价值；单次求解（UI 交互）反而因编译开销变慢 → 需「编译成本 / 预期重跑次数」的自适应开关。

## 6. 优先级 / ROI
- **P3 / 远景**，ROI 低–中（24 N11）。建议仅在确认「同题重跑」是主要场景（CI 每日全量）时才投入。

## 7. 实现思路
**方案 A（字节码 VM，轻）**：
```
// 1. 编译：puzzle → Vec<CheckOp>
//    CheckOp = {VerifyShapePool, VerifyRing{vertices}, VerifyArea{targets}, ...}
//    只包含 puzzle.rules 中启用的规则；常量内联（H,W,area 目标）
// 2. 执行：解释器遍历 CheckOp（无动态规则查找）
```
**方案 B（真 codegen，重）**：生成 Rust 源码 → `rustc` 动态编译 → `dlopen`；或 LLVM ORC JIT。收益更大但工程重、可移植性差。
- 自适应：记录同题 hash 的重跑次数 > 阈值才启用特化。

## 8. 验证方法
- 等价性：特化执行 vs 通用执行，在同一批题上解完全一致（关键红线）。
- `--baseline` REGRESSION=0 + wall 对比（需扣除编译时间）。

## 9. 依赖与前置
- 前置：[27-corpus-cache.md](27-corpus-cache.md)（先做缓存，收益更直接）。
- 协同：[19-simd-flood.md](19-simd-flood.md)、[12-low-allocator.md](12-low-allocator.md)（同为常数因子优化，可叠加）。

## 10. 参考
- `docs/优化/24` §11.11；`lib.rs:41`（OnceLock 缓存雏形）；`types.rs:123`。
