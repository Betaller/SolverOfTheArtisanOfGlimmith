# 07 · 轻量冲突学习 / nogood

> 状态：🟢 新方向 ｜ 分类：机器学习 ｜ 来源：`docs/优化/24` §5.3
> 关联：[06-ml-ordering.md](06-ml-ordering.md) · [25-diff-fuzz.md](25-diff-fuzz.md)

## 1. 一句话
CDCL 的核心优势是「学冲突子句」（`11` §2.3 已点出 DLX 无学习是短板）。可在一个模块内维护「冲突特征 → 禁止」的轻量 nogood 表（per-rule-type 冲突簇），部分补上 SAT 的学习能力。

## 2. 思想（为什么有效）
- 手写 DFS 每次回溯后「忘记」了为什么冲突；SAT 的 clause learning 把「导致冲突的赋值组合」记下来，以后跳过。本题可类比：当某形状/边赋值组合反复导致回溯，记为一个 nogood（反向索引跳过共享冲突节点的兄弟形状，参考 `11` §4.3 *conflict-driven shape skipping*，⭐⭐）。
- ML 角度：用历史冲突训练「哪些形状/边组合易冲突」，预存为 nogood，减少重复探索。

## 3. 现状与代码位置
- 冲突点：aog `check_*` 系列（`search.rs`）、backtrack `check_sealed_regions`（`backtrack.rs:750`）等剪枝返回 `false` 处。
- 当前无跨分支记忆：每次回溯纯局部撤销。

## 4. 收益
- 把「无学习手写 DFS」部分补上 SAT 的学习能力——`11` 论证这是极硬实例上超越手写 DFS 的关键。
- 对档③/⑤ 反复撞同一冲突的题尤其有效。

## 5. 代价与风险
- **风险：中**。nogood 库膨胀需 cap + LRU；错误 nogood 会误杀正确解（须保证 nogood 是** sound **：仅禁止已证冲突的组合）。
- **代价**：中（~200–400 行 + 冲突特征提取）。

## 6. 优先级 / ROI
- **P2**，ROI 中（与 [06](06-ml-ordering.md) 互补，属「学习能力」主线）。

## 7. 实现思路
```
// 模块内 nogood 缓存（按 rule 类型分桶）
struct NoGoodSet { by_rule: HashMap<RuleType, LruCache<ConflictKey, ()>> }
// 冲突特征：引起 false 的 (cell, shape_index) 或 (edge, CutState) 组合
fn on_conflict(feature: &ConflictKey, rule: RuleType) {
    nogood.by_rule[rule].insert(feature.clone(), ());
}
// 分支前：若该 (cell,shape) 在 nogood 中 → 跳过（除非被更高优先证据推翻）
```
- 冲突特征须**局部 sound**（不依赖全局状态），否则误杀。
- 结合 [06](06-ml-ordering.md)：用训练得到的「易冲突组合」预填 nogood。

## 8. 验证方法
- `--baseline` REGRESSION=0 是硬红线（nogood 误杀会立刻现形）。
- 单测：构造已知冲突题，断言 nogood 不禁止正确解。

## 9. 依赖与前置
- 依赖 [25-diff-fuzz.md](25-diff-fuzz.md) 做 soundness 回归守卫。
- 参考 `11` §4.3 conflict-driven shape skipping（AoG_Solver）。

## 10. 参考
- `docs/优化/24` §5.3；`11` §2.3（CDCL）/§4.3；SAT clause learning 文献。
