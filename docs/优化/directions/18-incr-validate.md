# 18 · 增量验证

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N1）｜ 来源：`docs/优化/24` §11.3
> 关联：[11-low-doublevalidate.md](11-low-doublevalidate.md) · [10-low-bitvector.md](10-low-bitvector.md) · [16-incr-solve.md](16-incr-solve.md)

## 1. 一句话
`validate::validate` 每次**全量扫 22 条规则**；若已知「仅第 k 个区域是本次新定的」，只需验证该区域 + 其邻接边/顶点/邻居区域，复杂度从 O(HW) 降到 O(变更区域)。

## 2. 思想（为什么有效）
- 22 条规则里绝大多数是**局部**的：per-region（shape_pool/precise/area/range/block/non_block/solitary/compass/puzzle_piece）、per-edge（heterogeneous/homogeneous/mixed/differentiation/inequality/difference）、per-vertex（ring/brick/watchtower）。只有 `same`/`different`/`rose_window` 是全局比较（见 `13-官方语料二级结论` 的规则分类）。
- 因此「新增一个区域」只会影响：该区域自身的 per-region 规则、它与邻接区域的 per-edge/per-vertex 规则、以及 `different` 的去重表。**其余全盘不需重扫。**
- aog 是**逐区域构造**（`search.rs`），`pieces.rs:590` `reconstruct_and_validate` 也是逐个候选重建——它们天然携带「本次新增区域」信息，可直接喂给增量验证。

## 3. 现状与代码位置
- 全量验证：`rsolver/src/solver/validate.rs:14` `validate`（每次整盘）。
- 内部已验证：`pieces.rs:590-667` `reconstruct_and_validate`（逐个候调整体验）。
- 出口再验：`mod.rs:304` `build_solution`（见 [11](11-low-doublevalidate.md)）。
- 区域构造顺序：`aog/search.rs` DFS 逐区域。

## 4. 收益
- 在 [11](11-low-doublevalidate.md) 去掉「双重全量」之后，进一步把「单次全量」降为增量 —— 秒级题与批量 CI 显著。
- 让「每次放置后即时验证」变得可行（当前因太贵只在叶子做），从而**提前发现矛盾**（等价于增强剪枝）。

## 5. 代价与风险
- **风险：低**（正确性由最终全量兜底：增量用于搜索中剪枝，出口仍可全量一次）。
- **代价**：小–中（~150–300 行：区域级/边级/顶点级验证入口 + 影响域计算）。

## 6. 优先级 / ROI
- **P1**，ROI 高（速赢；24 N1）。

## 7. 实现思路
```
// validate.rs 新增
fn validate_region(puzzle, regions, new_rid) -> bool {
    // 1. per-region 规则：shape_pool/precise/area/range/block/non_block/solitary/compass/puzzle_piece
    // 2. per-edge：遍历 new_rid 区域的边界边 → heterogeneous/homogeneous/mixed/differentiation/inequality/difference
    // 3. per-vertex：遍历该区域角点 → ring/brick/watchtower
    // 4. different：新形状键 vs 已有键集合（O(1) 哈希）
}
// DFS 中：每放一个区域即 validate_region(...)；通过才继续
// 出口：validate(...) 全量一次（兜底）
```
- 影响域：区域邻接表（`mod.rs` 已有边/顶点结构可 O(1) 索引）。

## 8. 验证方法
- 等价性：随机题上「增量每次验」与「叶子全量验」结果一致。
- `--baseline` REGRESSION=0。

## 9. 依赖与前置
- 前置：[11-low-doublevalidate.md](11-low-doublevalidate.md)（先去双重，再转增量）。
- 协同：[10-low-bitvector.md](10-low-bitvector.md)（邻接/顶点 O(1) 索引）、[16](16-incr-solve.md)（交互式）。

## 10. 参考
- `docs/优化/24` §11.3；`validate.rs:14`；`13-官方语料二级结论`（规则局部分类）。
