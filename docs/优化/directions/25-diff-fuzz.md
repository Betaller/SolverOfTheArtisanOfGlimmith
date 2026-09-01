# 25 · 差分 / fuzz 验证

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N10，鲁棒性）｜ 来源：`docs/优化/24` §11.10
> 关联：[07-ml-nogood.md](07-ml-nogood.md) · [14-meta-determinism.md](14-meta-determinism.md) · [18-incr-validate.md](18-incr-validate.md)

## 1. 一句话
用差分测试（Rust 求解器 vs 参考求解器 / vs 自身不同配置）+ fuzzing（随机 puzzle + 边界组合）自动抓「剪枝误杀正确解」「非终止/崩溃」，并把关键剪枝的 soundness 做成不变量断言 —— 把「优化→全量回归」的 30 分钟循环前置为秒级。

## 2. 思想（为什么有效）
- 当前保证「优化不引入回归」的手段只有**全量 benchmark**（2488 题、数十分钟），且噪声大（[14-meta-determinism.md](14-meta-determinism.md)）。
- 更根本的守卫是**剪枝 soundness**：每一个剪枝（sealed-region、vertex ring/brick、nogood、增量验证）都必须满足「永不放过任何正确解」。这是**可单元测**的不变量，不必靠全量回归偶然发现。
- 差分测试：同一题用「关闭某剪枝」与「开启」两配置求解，若关闭版能解出而开启版不能 → 该剪枝误杀（回归！）。这是**自动定位误杀剪枝**的强力手段。
- fuzzing：随机生成 puzzle（含边界情况：2×2 最小盘、全 blocked、满 pre-boundary、矛盾线索）找崩溃/挂死（`17` 提到 deadline 盲区与挂死）。

## 3. 现状与代码位置
- 剪枝点：`backtrack.rs:750` `check_sealed_regions`、`:1243` `check_vertex_ring_ok`、`prototypes.rs:139` `sat_boundary_feasible`、`aog/search.rs` 各 `check_*`。
- 无「按剪枝开关」的配置开关（无法做差分）。
- `17` 记录了挂死根因与 deadline 盲区 —— 说明确实存在鲁棒性问题。
- 测试：`tests/`（365 tests，`AGENTS.md`），无 fuzz。

## 4. 收益
- 把回归检测从「全量 30 分钟」前置到「单元/差分 秒级」，加速所有后续优化迭代。
- 直接守卫 [07-ml-nogood.md](07-ml-nogood.md)（nogood 误杀）、[18](18-incr-validate.md)（增量验证漏检）、[20](20-active-decomp.md)（分解剪掉正解）等高风险新方向。

## 5. 代价与风险
- **风险：低**（纯测试基建，不改求解逻辑）。
- **代价**：中（剪枝开关 ~100 行 + 差分脚本 ~200 行 + fuzz harness ~200 行）。

## 6. 优先级 / ROI
- **P1**，ROI 高（是多个高风险新方向的安全网；24 N10）。

## 7. 实现思路
```
// 1. 剪枝开关：env / Puzzle 级 flags
struct PruneFlags { sealed: bool, vertex_ring: bool, sat_boundary: bool, bf: bool, ... }
//   默认全开；差分测试逐个关闭
// 2. 差分脚本 scripts/diff_prune.py：
//    对每道官方题，跑 (全开) vs (关第 i 个剪枝)
//    若 关i 能解出 而 全开 不能 → 报告「剪枝 i 误杀」+ 题号
// 3. fuzz harness：随机 puzzle 生成（尺寸 2..16、随机规则子集、随机线索）
//    → rsolver 跑，断言：不 panic、不超时(deadline 生效)、解过 validate
// 4. soundness 单测：对每个剪枝，构造「已知正解」的题，断言剪枝不拒绝它
```

## 8. 验证方法
- 差分脚本应在**当前基线**上报告 0 误杀（若有，说明已有 bug —— 本身就是收获）。
- fuzz 跑 N 轮无 panic / 无 deadline 失效。

## 9. 依赖与前置
- 需给剪枝加开关（小改，但触及热路径，注意零开销抽象）。
- 被依赖：[07-ml-nogood.md](07-ml-nogood.md)、[18-incr-validate.md](18-incr-validate.md)、[20-active-decomp.md](20-active-decomp.md)、[21](21-deduction-engine.md)。

## 10. 参考
- `docs/优化/24` §11.10；`17-挂死根因与deadline盲区`；`11` §6（已证伪清单，需不变量守卫）。
