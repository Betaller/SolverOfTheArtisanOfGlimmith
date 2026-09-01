# 31 · 子问题可行性记忆表

> 状态：🟢 新方向 ｜ 分类：搜索 / 记忆化 ｜ 来源：本文新调研（24 未覆盖）
> 关联：[19-simd-flood.md](19-simd-flood.md) · [27-corpus-cache.md](27-corpus-cache.md) · [20-active-decomp.md](20-active-decomp.md)

## 1. 一句话
`11` §3.6 断言「记忆化/DP 子状态去重不可行」（状态空间 256^256）—— 这指的是**全局状态**记忆化。但**子问题级**可行性是可以记忆的：「这块空区域能否被形状池 P 划分」只依赖 (区域形状, 剩余格数, 约束签名)，可缓存命中，直接砍掉重复的可行性判定。

## 2. 思想（为什么有效）
- 全局状态记忆化不可行，因为状态数天文。但可行性判定有大量**重复子问题**：
  - `empty_area_check`（`aog/search.rs:1265`，全盘 flood-fill，O(cells²) 主热点）反复判断「剩余空区是否仍可划分」。
  - `pieces` 的「某形状能否放入」、`rose` 的「某候选区域是否可行」。
- 这些子问题的**输入是可压缩的**：(空区轮廓的规范键, 剩余面积, 启用的形状/面积约束签名)。不同搜索分支常常落到同一个子问题签名。
- 命中缓存即可跳过昂贵的 flood-fill / 枚举 —— 把「每次都算」变成「查表」。

## 3. 现状与代码位置
- `aog/empty.rs` / `search.rs:1265` `empty_area_check`（每次放置都全盘 flood-fill）。
- `pieces.rs:207` `generate_all_placements`（每次重算候选）。
- `shapes.rs:32` `dihedral_key`（已有「规范键」思想，可复用为子问题签名的基础）。
- 无跨分支记忆（每次回溯后纯撤销，见 [07-ml-nogood.md](07-ml-nogood.md)）。

## 4. 收益
- 直接命中 aog 最贵的单项（empty_area_check）与 pieces 候选生成 → 惠及 aog 首解的 **97% 题**（附录数据）。
- 与 [19-simd-flood.md](19-simd-flood.md) 正交：SIMD 是「算得更快」，记忆化是「不重复算」，可叠加。

## 5. 代价与风险
- **风险：中**。缓存键必须**完备**（漏掉任一影响可行性的因素 → 误判「不可行」而剪掉正确解）。可行性的依赖因素比解的判定更多（边界、blocked、剩余约束…），这是主要风险。
- **代价**：中（~200–400 行：签名计算 + LRU + 失效策略）。签名本身有计算成本，需权衡。

## 6. 优先级 / ROI
- **P2**，ROI 中高（命中率高则收益大；但签名完备性风险需 [25-diff-fuzz.md](25-diff-fuzz.md) 守卫）。

## 7. 实现思路
```
// 签名：只取"影响可行性"的量
struct FeasKey { area_shape_key: u64,   // 空区轮廓的 dihedral 规范键
                 remaining: u32,        // 剩余格数
                 rule_sig: u32 }        // 启用的形状/面积约束签名（bitmask）
// 缓存
feas_cache: LruCache<FeasKey, bool>     // true=可划分
// 调用点（empty_area_check 之前）
if let Some(&ok) = feas_cache.get(&key) { return ok; }
let ok = expensive_partition_check(...);
feas_cache.insert(key, ok);
```
- **保守落地**：先用缓存只存 `false`（不可行的判定）—— 因为「不可行」若误判会剪掉正解，风险高；改为**只缓存 true（可行）**的旁路：命中 true 直接放行，未命中照常算。这样即使签名不完备，最坏是「没命中」（无收益），不会误剪。
- 逐步：先只读 true-cache 验证命中率，确认安全后再考虑缓存 false。

## 8. 验证方法
- 命中率统计（先做只读埋点，测真实命中率再决定是否落地）。
- soundness：用 [25-diff-fuzz.md](25-diff-fuzz.md) 的差分（开/关记忆表）验证解集完全一致。
- `--baseline` REGRESSION=0。

## 9. 依赖与前置
- 前置：[19-simd-flood.md](19-simd-flood.md)（空区轮廓计算可复用）、[25-diff-fuzz.md](25-diff-fuzz.md)（soundness 守卫）。
- 协同：[27-corpus-cache.md](27-corpus-cache.md)（跨题持久化记忆表）。

## 10. 参考
- `11` §3.6（全局记忆化不可行的论证）；`19-simd-flood.md`；`aog/search.rs:1265`。
