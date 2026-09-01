# 11 · 去双重 validate

> 状态：🟢 新方向 ｜ 分类：底层系统调优 ｜ 来源：`docs/优化/24` §6.3
> 关联：[18-incr-validate.md](18-incr-validate.md) · [10-low-bitvector.md](10-low-bitvector.md)

## 1. 一句话
`pieces.rs:667` 在 `reconstruct_and_validate` 内已调一次 `validate::validate`，`mod.rs:304` 的 `build_solution` 又调一次——每道被 pieces（及 rose/edge_csp/backtrack）解出的题，22 条规则全量扫描跑**两遍**。去掉重复，秒级题省一遍完整验证。

## 2. 思想（为什么有效）
- `validate::validate`（`validate.rs:14`）是独立全量验证门，成本 O(HW) 扫 22 规则。
- 模块内部 `reconstruct_and_validate`（`pieces.rs:590-667`）已用 `validate` 验证每个候选 → 返回的 `ModuleOutcome` 已是「validated」。但 `build_solution`（`mod.rs:291-304`）出口**再**调一次 `validate` 才返回 `Solution`。
- aog 走 `build_solution_trusted`（`mod.rs:91,344`）——**跳过**全量 validate，只做边界一致性快速复查。故「双重」只命中 pieces/rose/edge_csp/backtrack 路径。

## 3. 现状与代码位置
- 内部：`pieces.rs:667` `if !validate(puzzle, &regions)`。
- 出口：`mod.rs:304` `build_solution` 内 `if !validate(puzzle, &regions)`。
- aog：`mod.rs:91,344` `build_solution_trusted`（无二次全量）。
- 注释 `mod.rs:300-301` 说明 build_solution 非 trusted（router 再验），故重复。

## 4. 收益
- 对「被非-aog 模块解出」的题（当前少数，但并发 [01](01-parallel-inter.md) 后占比会升），省一遍全量 22-规则扫描 → 秒级题显著。
- 与 [18-incr-validate.md](18-incr-validate.md) 叠加：进一步把「单次全量」降为增量。

## 5. 代价与风险
- **风险：低**（正确性由 aog 的 `build_solution_trusted` 模式证明：内部已验证即足够）。
- **代价**：小（~30–60 行：把 pieces/rose/edge_csp/backtrack 的 `build_solution` 改为 `build_solution_trusted` 或仅做边界一致性复查）。

## 6. 优先级 / ROI
- **P1**，ROI 中（速赢，24 §8 速赢 S4）。

## 7. 实现思路
```
// mod.rs：非-aog 模块也走 trusted
return build_solution_trusted(regions, &start, puzzle, "pieces", attempts);
// build_solution_trusted 当前只做 boundary-consistency 复查（validate.rs:48-70 的等价快查），不再跑全 22 规则
```
- 若担心「模块内部 validate 有 bug 漏检」：保留 router 侧 `IndependentValidator`（`base.py:113`）作为最终门（Python 层已独立验证），双保险但不在 Rust 热路径重复。

## 8. 验证方法
- `--baseline` REGRESSION=0（解仍过 Python `IndependentValidator` 最终门）。
- 单测：断言 `build_solution_trusted` 与旧双验结果一致（同题同解）。

## 9. 依赖与前置
- 深化：[18-incr-validate.md](18-incr-validate.md)（增量验证）。

## 10. 参考
- `docs/优化/24` §6.3；`mod.rs:91,304,344`；`pieces.rs:667`。
