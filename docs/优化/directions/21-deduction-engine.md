# 21 · 预搜索 deduction engine

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N4）｜ 来源：`docs/优化/24` §11.6
> 关联：[20-active-decomp.md](20-active-decomp.md) · [16-incr-solve.md](16-incr-solve.md) · [18-incr-validate.md](18-incr-validate.md)

## 1. 一句话
把 `11` §3.3 提出的六步传播循环（boundary→vertex→area→connectivity→forced→fence）**实现为预搜索阶段的统一不动点引擎**，在 DFS 开始前尽量「钉死」确定信息，把「边搜边推」改成「先推后搜」，显著降低搜索树宽度。

## 2. 思想（为什么有效）
- DPLL/CDCL 的核心范式：**unit propagation 到不动点 → 检查全定/矛盾 → 决策分支 → 回到传播**（`11` §3.3）。当前实现是「边搜边推」：BF 面积传播每 256 步、SAT 边界可行性每 64 步 —— 都是**间歇性、战略性妥协**。
- 改成预搜索 + 事件驱动：在搜索开始前把所有能推的都推到不动点，搜索树的根就已经被大幅收紧（很多格/边/区域已确定），分支因子直接下降。
- 这是「让搜索树更窄」的**广谱**手段，对多规则组合题（失败主因，`21` §1.2 指出绝大多数 FAIL 是 2–4 规则组合）尤其有效。

## 3. 现状与代码位置
- 已有零散实现：`prototypes.rs:13` Bellman-Ford 面积传播（每 256 步，`backtrack.rs:487`）、`:92` GF(2) 奇偶、`:139` `sat_boundary_feasible`（每 64 步，`backtrack.rs:506`）。
- `edge_csp/prop.rs:51` 已有**完整的 AC-3 式不动点传播**（10+ propagator + failed-literal probing）—— 但**只对 edge_csp-capable 题生效**（`mod.rs:157`），非 edge 题享受不到。
- `11` §3.3 给出 `deduction_loop` 伪代码：6 步传播，当前只实现 2/3（SAT 边界、BF 面积），1/4/5/6 未完成。

## 4. 收益
- 广谱降低分支因子 → aog（首解 97%）与 rose/edge_csp 都受益。
- 副产品：推断出的 forced 边直接喂给 [20-active-decomp.md](20-active-decomp.md) 做降维分治。
- 预搜索矛盾可**秒级判无解**（不必进搜索）。

## 5. 代价与风险
- **风险：中**。传播顺序/不动点正确性必须严格（错误的「钉死」会剪掉正确解）；每步传播器须独立 sound 测试。
- **代价**：中（~500–900 行：6 个 propagator + 不动点调度 + 与现有引擎融合）。

## 6. 优先级 / ROI
- **P1**，ROI 高（广谱 + 是 [20](20-active-decomp.md)/[16](16-incr-solve.md) 的前置；24 N4）。

## 7. 实现思路
```
// solver/deduce.rs（新增）
pub fn deduction_loop(state: &mut DeduceState) -> Result<(), Contradiction> {
    loop {
        let mut changed = false;
        changed |= propagate_boundary_from_regions(state)?;  // 区域 ID 不同 → 边界定（步骤1）
        changed |= propagate_vertex_degree(state)?;          // ring/brick 度约束（步骤2，已有雏形）
        changed |= propagate_area_constraints(state)?;        // 密封面积 → 邻居上下界（步骤3，BF 已有）
        changed |= propagate_connectivity(state)?;            // NonBoundary → DSU 合并（步骤4）
        changed |= propagate_forced_cells(state)?;            // 只剩1个可达区域 → 强制分配（步骤5）
        changed |= propagate_fence_bits(state)?;              // arm_count 超限 → 矛盾（步骤6）
        if !changed { break; }                                // 不动点
    }
    Ok(())
}
// 调用点：mod.rs 的 pre_search_topology_check 之后、aog DFS 之前
```
- **关键边界**（`11` §3.4）：步骤 4 的 DSU 合并**只对 ring+brick 推导出的 NonBoundary 成立**，对 fence **不成立**（dihedral 类不固定具体边，已证伪 puzzle 0390）。
- 复用 `edge_csp/prop.rs` 的 propagator（其 `probe`/failed-literal 是高质量实现），抽到公共层给所有题用。

## 8. 验证方法
- 每步 propagator 独立单测（soundness：不禁止任何正确解）。
- 用 `-answer` 语料校验：断言「被强制的边/格」在官方解中确实如此。
- `--baseline` REGRESSION=0。

## 9. 依赖与前置
- 复用 `prototypes.rs` / `edge_csp/prop.rs` 现有 propagator。
- 被依赖：[20-active-decomp.md](20-active-decomp.md)、[16-incr-solve.md](16-incr-solve.md)。

## 10. 参考
- `docs/优化/24` §11.6；`11` §3.3（deduction_loop 伪代码）、§3.7；`prototypes.rs`；`edge_csp/prop.rs:51`。
