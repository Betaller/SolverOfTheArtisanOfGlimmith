# 02 · 模块内并行 DFS / work-stealing

> 状态：🟢 新方向 ｜ 分类：并行化 ｜ 来源：`docs/优化/24` §4.2
> 关联：[01-parallel-inter.md](01-parallel-inter.md) · [19-simd-flood.md](19-simd-flood.md)

## 1. 一句话
每个求解模块内部是单线程 DFS；其**枚举 / 生成阶段**（aog 形状 slash-distance 元组、rose per-seed 候选 BFS、pieces 放置枚举）天然无依赖可并行；搜索树本身可用 work-stealing 多核深搜，直接对抗 40s 超时档（档⑤ 根本难）。

## 2. 思想（为什么有效）
- 搜索树是「可分割」的：从根出发的不同分支彼此独立。把 DFS 节点压入工作队列，N 线程各取一个子树深搜；某线程找到解即全局取消。这与现代 SAT/CP 求解器的并行化思路一致。
- **枚举并行是低风险甜点**：`generate_all_placements`（`pieces.rs:207`）、rose 候选 BFS（`region_match.rs:53`）、aog `place_non_predifined_shape` 的 slash-distance 元组循环（`search.rs:647-708`）产出的是**独立候选集合**，无撤销栈依赖，用 rayon `par_iter` 即可。

## 3. 现状与代码位置
- aog：`rsolver/src/solver/aog/search.rs:854` DFS；`search.rs:647-708` slash-distance 元组枚举。
- rose：`rsolver/src/solver/rose/region_match.rs:53` per-seed 候选生成。
- pieces：`rsolver/src/solver/pieces.rs:207` `generate_all_placements`。
- backtrack：`rsolver/src/solver/backtrack.rs:463` 区域生长 DFS（撤销用 `cell_to_region`/`region_shapes` push/pop）。

## 4. 收益
- 大网格（档⑤，1434 类 15×15）可在多核上近似线性加速，把"差一点超时"的题推过阈值。
- 枚举并行（低风险部分）即可缩短 pieces/rose 的候选准备墙钟。

## 5. 代价与风险
- **风险：中**。撤销状态线程安全是核心难点：backtrack 的 `push/pop`、`edge_csp` 的 `changed` trail 是线程局部可变状态。并行搜索需改成「路径拷贝 + 比较」或 thread-local 撤销栈 + 工作窃取。
- **确定性丢失**：先找到解的路径不确定（对官方解比对 DIFF 无影响，解唯一），但回归比对需 `--retry-timeouts` 单独确认（`14-meta-determinism.md`）。
- **代价**：中–大（~500 行 + 引入 rayon 依赖）。

## 6. 优先级 / ROI
- **P2**，ROI 中（仅对大网格/档⑤有意义；小盘并行反而因同步开销变慢）。建议先做枚举并行（低风险），搜索树并行留待档⑤ 攻坚。

## 7. 实现思路
**阶段 1（枚举并行，低风险）**：把 `generate_all_placements` / rose 候选 BFS 的循环体改为 `rayon::par_iter` 收集候选；结果汇入同一 `Vec`（加锁或分块收集）。
**阶段 2（搜索树 work-stealing）**：
```
struct WorkQueue<T> { deque: SegQueue<SearchNode> }
// 每线程 pop 一个节点深搜；找到解 → 设置 AtomicBool cancel
// 撤销：节点带「从根到该节点的赋值路径」拷贝（Cow），而非共享可变 state
```
或采用「stack-stealing」：每个 DFS 在深度 d 把当前栈快照推入共享队列，空闲线程接管。

## 8. 验证方法
- `--baseline` 对比：要求 REGRESSION=0（解唯一，并行只改路径）；关注档⑤ 题 NEW 解出数。
- 单线程 vs 多线程同一 seed 应得**同一解**（唯一解保证），用 `matches_official` 兜底。

## 9. 依赖与前置
- 依赖 [01-parallel-inter.md](01-parallel-inter.md) 的取消原语可复用。
- 建议先落地枚举并行，再评估搜索树并行的边际收益。

## 10. 参考
- `docs/优化/24` §4.2；`11` §4.5C（谜题内并行少见）；现代 SAT 并行（cube-and-conquer）。
