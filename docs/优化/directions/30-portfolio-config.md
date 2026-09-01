# 30 · 参数多样化组合（配置级 portfolio）

> 状态：🟢 新方向 ｜ 分类：并行化 / 组合 ｜ 来源：本文新调研（24 未覆盖）
> 关联：[01-parallel-inter.md](01-parallel-inter.md) · [29-adapt-budget.md](29-adapt-budget.md) · [08-ml-restart.md](08-ml-restart.md)

## 1. 一句话
不只是「多个不同模块并发跑」，而是**同一个模块用不同参数配置**并发跑（不同 shape_cap、不同启发权重、不同时间切片、不同变量序策略）——算法组合（algorithm portfolio）的经典手段，常比「换算法」更有效。

## 2. 思想（为什么有效）
- 算法组合理论（Rice 1976 / SAT Competition 观察）：没有任何单一配置在所有实例上最优，但**少数几个配置的并联**往往能覆盖绝大多数实例。收益来自「配置-实例」匹配的互补性，而非单一配置的改进。
- 本题有多个天然可变的旋钮：
  - `AOG_SHAPE_CAP`（`aog/types.rs:75`，当前默认 0=无限，是 OOM 根因）
  - `AOG_ROSE_BUDGET_MS`（`mod.rs:381`，rose 题给 aog 的 3s 预算）
  - BF 传播频率（每 256 步，`backtrack.rs:487`）、SAT 边界检查频率（每 64 步，`backtrack.rs:506`）
  - 变量序策略（MRV / 度启发 / row-major）、值序（先 join 邻居 vs 先开新区）
  - rose `CANDIDATE_CAP=20000`、pieces `MAX_AREA_TARGET=12` / `MAX_COMPASS_PLACEMENTS=2000`
- 这些参数当前是**手调的固定常数**，对不同题显然不是最优。并发跑 2–4 组配置，first-wins，等于「用算力换参数调优」。

## 3. 现状与代码位置
- 串行单配置：`mod.rs:16` 每模块一次调用，参数写死。
- 可调旋钮：`aog/types.rs:75`（SHAPE_CAP）、`mod.rs:381`（AOG_ROSE_BUDGET_MS）、`backtrack.rs:487,506`（传播频率）、`pieces.rs:31,35`（候选上限）、`rose` CANDIDATE_CAP。
- 并发基建：未落地（[01](01-parallel-inter.md)）。

## 4. 收益
- 对「当前参数恰好不合适」的题（很可能是超时档的一部分）直接解出。
- **零算法改动风险**：每个配置都是现有代码路径，只是参数不同。
- 与 [01](01-parallel-inter.md) 模块并发正交，可叠加（模块 × 配置 的笛卡尔组合，按预算裁剪）。

## 5. 代价与风险
- **风险：低**（纯调度 + 参数注入）。
- **代价**：小–中（参数注入接口 ~100 行 + 配置选择策略 ~100 行）。核心难点是**选哪几组配置**（见实现思路）。

## 6. 优先级 / ROI
- **P1–P2**，ROI 中–高（依赖并发基建；一旦 [01](01-parallel-inter.md) 落地，这是几乎免费的增益）。

## 7. 实现思路
```
// 1. 参数注入：把硬编码常数改为 Config 结构体
struct SolveConfig { shape_cap: usize, bf_every: u32, sat_every: u32,
                     var_order: VarOrder, val_order: ValOrder, ... }
// 2. 配置组（离线用历史数据挑出的 3-4 组"互补"配置）
const PORTFOLIO: [SolveConfig; 4] = [ /* aggressive / conservative / area-first / shape-first */ ];
// 3. 并发：对有能力模块 × 配置的组合，按预算上限并发，first-wins
// 4. 配置选择：用 [05-ml-routing.md](05-ml-routing.md) 的模型输出 top-k 配置（而非单配置）
```
- 配置挑选方法：跑一次全量基准，对每题记录「哪组配置先解出」→ 用 set-cover 贪心挑最小的互补配置集（常 3–5 组就覆盖 ~95%）。

## 8. 验证方法
- 离线：统计「单配置 vs 组合」的覆盖曲线（配置数 → 解出题数），确认边际收益。
- `--baseline` REGRESSION=0；关注超时档 NEW 解出。

## 9. 依赖与前置
- 强前置：[01-parallel-inter.md](01-parallel-inter.md)（并发 first-wins 框架）。
- 协同：[29-adapt-budget.md](29-adapt-budget.md)（配置间预算分配）、[08](08-ml-restart.md)（随机重启可视为「同一配置不同 seed」的特例）。

## 10. 参考
- Rice (1976) 算法选择；SAT/CP 组合求解（portfolio）实践；`mod.rs:381`；`aog/types.rs:75`。
