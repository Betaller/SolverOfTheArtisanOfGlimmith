# 04 · GPU offload（远景）

> 状态：⚪ 低优先级 ｜ 分类：并行化 ｜ 来源：`docs/优化/24` §4.4
> 关联：[02-parallel-intra.md](02-parallel-intra.md) · [19-simd-flood.md](19-simd-flood.md) · [24-distributed.md](24-distributed.md)

## 1. 一句话
连通性 flood-fill / 形状库匹配是「多独立小任务」形态，理论上可 GPU 批处理；但当前瓶颈是强依赖搜索树，GPU 性价比低，**不推荐**作为近期方向，仅作为远景记录。

## 2. 思想（为什么有效 / 为什么谨慎）
- GPU 适合「海量独立同构任务」：形状库匹配（每形状独立比对）、候选枚举、位并行连通性（`19-simd-flood.md`）在 GPU 上可万级并行。
- 但本求解器瓶颈是**搜索树**（强依赖、需撤销、难并行），非规整矩阵运算。把 DFS 搬上 GPU 需把撤销栈 / 递归改成 GPU 友好的显式栈 + 全局内存，得不偿失。

## 3. 现状与代码位置
- 当前全 CPU：`rsolver/src/solver/**`，无 GPU 代码。`wasm.rs` 仅作浏览器编译目标，非 GPU。
- 唯一可 GPU 化的「规则内核」若未来做 SAT/ILP 后端（见 `22`），其 propagate 阶段才有 GPU 价值。

## 4. 收益
- 仅对档⑤ 极端题的「形状枚举 / 候选生成」阶段有潜在加速（配合 [24-distributed.md](24-distributed.md)）。

## 5. 代价与风险
- **风险：高（工程）**。Rust→GPU 需重写核心（wgpu/cuda）、丢失现有 deadline 控制、调试困难。
- **代价**：大（数千行 + 新依赖 + CI 需 GPU  runner）。

## 6. 优先级 / ROI
- **⚪ 低优先级 / 不推荐**。标记为「远景」避免重复投入。

## 7. 实现思路（仅记录，不实现）
- 把 `shapes.rs` 形状匹配、`pieces.generate_all_placements` 枚举迁到 GPU kernel，CPU 侧收集结果再进 DFS。
- 或：若未来做 SAT 后端，用 GPU 跑单元传播批量。

## 8. 验证方法
- 仅作实验性分支，不与主线比对。

## 9. 依赖与前置
- 前置：[19-simd-flood.md](19-simd-flood.md)（CPU SIMD 先做，GPU 是更远的延伸）。

## 10. 参考
- `docs/优化/24` §4.4；`11` §4.5C；现代 SAT GPU 研究（非主流）。
